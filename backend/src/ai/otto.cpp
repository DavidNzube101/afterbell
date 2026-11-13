#include "ai/otto.hpp"

#include <curl/curl.h>

#include <arpa/inet.h>
#include <netdb.h>
#include <sys/socket.h>

#include <algorithm>
#include <chrono>
#include <cctype>
#include <deque>
#include <memory>
#include <mutex>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string_view>
#include <thread>
#include <unordered_map>
#include <utility>
#include <vector>

namespace afterbell::ai {
namespace {

constexpr std::size_t kMaxMessages = 16;
constexpr std::size_t kMaxMessageChars = 4000;
constexpr std::size_t kMaxTotalChars = 18000;
constexpr std::size_t kRequestsPerMinutePerAccount = 12;

using CurlPtr = std::unique_ptr<CURL, decltype(&curl_easy_cleanup)>;
using HeaderPtr = std::unique_ptr<curl_slist, decltype(&curl_slist_free_all)>;
using UrlPtr = std::unique_ptr<CURLU, decltype(&curl_url_cleanup)>;

struct RequestTarget {
    std::string url;
    std::string resolve;
};

struct StreamState {
    drogon::ResponseStream *stream = nullptr;
    std::string lineBuffer;
    long status = 0;
    bool disconnected = false;
    bool doneSent = false;
};

std::mutex rateMutex;
std::unordered_map<std::string, std::deque<std::chrono::steady_clock::time_point>> recentRequests;

bool accountRequestAllowed(const std::string &accountId) {
    const auto now = std::chrono::steady_clock::now();
    std::lock_guard<std::mutex> lock(rateMutex);
    for (auto it = recentRequests.begin(); it != recentRequests.end();) {
        auto &times = it->second;
        while (!times.empty() && now - times.front() >= std::chrono::minutes(1)) times.pop_front();
        if (times.empty()) it = recentRequests.erase(it);
        else ++it;
    }
    auto &times = recentRequests[accountId];
    while (!times.empty() && now - times.front() >= std::chrono::minutes(1)) times.pop_front();
    if (times.size() >= kRequestsPerMinutePerAccount) return false;
    times.push_back(now);
    return true;
}

bool privateV4(std::uint32_t addr) {
    const auto a = (addr >> 24U) & 0xffU;
    const auto b = (addr >> 16U) & 0xffU;
    return a == 0 || a == 10 || a == 127 || a >= 224 ||
           (a == 100 && b >= 64 && b <= 127) ||
           (a == 169 && b == 254) || (a == 172 && b >= 16 && b <= 31) ||
           (a == 192 && (b == 0 || b == 168)) ||
           (a == 198 && (b == 18 || b == 19));
}

bool publicAddress(const sockaddr *address) {
    if (address->sa_family == AF_INET) {
        const auto *v4 = reinterpret_cast<const sockaddr_in *>(address);
        return !privateV4(ntohl(v4->sin_addr.s_addr));
    }
    if (address->sa_family == AF_INET6) {
        const auto *v6 = reinterpret_cast<const sockaddr_in6 *>(address);
        const auto *bytes = v6->sin6_addr.s6_addr;
        const bool allZero = std::all_of(bytes, bytes + 16, [](unsigned char b) { return b == 0; });
        const bool loopback = std::all_of(bytes, bytes + 15, [](unsigned char b) { return b == 0; }) && bytes[15] == 1;
        const bool uniqueLocal = (bytes[0] & 0xfeU) == 0xfcU;
        const bool linkLocal = bytes[0] == 0xfeU && (bytes[1] & 0xc0U) == 0x80U;
        const bool multicast = bytes[0] == 0xffU;
        const bool mappedV4 = std::all_of(bytes, bytes + 10, [](unsigned char b) { return b == 0; }) &&
                              bytes[10] == 0xffU && bytes[11] == 0xffU;
        return !allZero && !loopback && !uniqueLocal && !linkLocal && !multicast && !mappedV4;
    }
    return false;
}

std::string lower(std::string value) {
    std::transform(value.begin(), value.end(), value.begin(), [](unsigned char c) {
        return static_cast<char>(std::tolower(c));
    });
    return value;
}

std::string urlPart(CURLU *url, CURLUPart part) {
    char *value = nullptr;
    if (curl_url_get(url, part, &value, 0) != CURLUE_OK || value == nullptr) return {};
    std::string result(value);
    curl_free(value);
    return result;
}

RequestTarget byokTarget(const Json::Value &input) {
    auto base = input["base_url"].asString();
    while (!base.empty() && base.back() == '/') base.pop_back();
    UrlPtr url(curl_url(), curl_url_cleanup);
    if (!url || curl_url_set(url.get(), CURLUPART_URL, base.c_str(), 0) != CURLUE_OK)
        throw std::invalid_argument("Enter a valid OpenAI-compatible HTTPS base URL.");
    const auto scheme = lower(urlPart(url.get(), CURLUPART_SCHEME));
    const auto host = lower(urlPart(url.get(), CURLUPART_HOST));
    const auto port = urlPart(url.get(), CURLUPART_PORT);
    const auto path = urlPart(url.get(), CURLUPART_PATH);
    if (scheme != "https" || host.empty() || (!port.empty() && port != "443") ||
        !urlPart(url.get(), CURLUPART_USER).empty() ||
        !urlPart(url.get(), CURLUPART_PASSWORD).empty() ||
        !urlPart(url.get(), CURLUPART_QUERY).empty() ||
        !urlPart(url.get(), CURLUPART_FRAGMENT).empty() ||
        !(path.empty() || path == "/" || path == "/v1"))
        throw std::invalid_argument("Use an HTTPS API base URL on port 443, ending at the API root or /v1.");

    addrinfo hints{};
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    addrinfo *resolved = nullptr;
    const int rc = getaddrinfo(host.c_str(), "443", &hints, &resolved);
    if (rc != 0 || resolved == nullptr)
        throw std::invalid_argument("Could not resolve the BYOK API host.");
    std::unique_ptr<addrinfo, decltype(&freeaddrinfo)> addresses(resolved, freeaddrinfo);

    std::string pinnedIp;
    for (auto *it = resolved; it != nullptr; it = it->ai_next) {
        if (!publicAddress(it->ai_addr))
            throw std::invalid_argument("The BYOK API host resolves to a private or reserved network address.");
        char hostBuffer[NI_MAXHOST]{};
        if (getnameinfo(it->ai_addr, static_cast<socklen_t>(it->ai_addrlen), hostBuffer,
                        sizeof(hostBuffer), nullptr, 0, NI_NUMERICHOST) != 0)
            continue;
        if (pinnedIp.empty()) pinnedIp = hostBuffer;
    }
    if (pinnedIp.empty()) throw std::invalid_argument("Could not safely resolve the BYOK API host.");
    const auto resolveIp = pinnedIp.find(':') == std::string::npos
                               ? pinnedIp
                               : "[" + pinnedIp + "]";
    const auto endpoint = base + (path == "/v1" ? "/chat/completions" : "/v1/chat/completions");
    return {endpoint, host + ":443:" + resolveIp};
}

void sendEvent(StreamState &state, const std::string &event, const Json::Value &data) {
    if (state.disconnected || state.stream == nullptr) return;
    Json::StreamWriterBuilder writer;
    writer["indentation"] = "";
    const auto payload = "event: " + event + "\ndata: " + Json::writeString(writer, data) + "\n\n";
    state.disconnected = !state.stream->send(payload);
}

void consumeLine(StreamState &state, std::string line) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    if (line.rfind("HTTP/", 0) == 0) {
        std::istringstream statusLine(line);
        std::string protocol;
        statusLine >> protocol >> state.status;
        return;
    }
    if (state.status < 200 || state.status >= 300 || line.rfind("data:", 0) != 0) return;
    auto payload = line.substr(5);
    while (!payload.empty() && payload.front() == ' ') payload.erase(payload.begin());
    if (payload == "[DONE]") {
        if (!state.doneSent) {
            sendEvent(state, "done", Json::Value(Json::objectValue));
            state.doneSent = true;
        }
        return;
    }
    Json::CharReaderBuilder builder;
    Json::Value chunk;
    std::string errors;
    std::istringstream input(payload);
    if (!Json::parseFromStream(builder, input, &chunk, &errors)) return;
    const auto &choices = chunk["choices"];
    if (!choices.isArray() || choices.empty()) return;
    const auto &content = choices[0]["delta"]["content"];
    if (!content.isString() || content.asString().empty()) return;
    Json::Value data;
    data["text"] = content.asString();
    sendEvent(state, "delta", data);
}

std::size_t receiveHeaders(char *data, std::size_t size, std::size_t count, void *opaque) {
    const auto bytes = size * count;
    auto &state = *static_cast<StreamState *>(opaque);
    std::string line(data, bytes);
    if (line.rfind("HTTP/", 0) == 0) consumeLine(state, std::move(line));
    return bytes;
}

std::size_t receiveBody(char *data, std::size_t size, std::size_t count, void *opaque) {
    const auto bytes = size * count;
    auto &state = *static_cast<StreamState *>(opaque);
    if (state.status < 200 || state.status >= 300) return bytes;
    state.lineBuffer.append(data, bytes);
    std::size_t newline = 0;
    while ((newline = state.lineBuffer.find('\n')) != std::string::npos) {
        auto line = state.lineBuffer.substr(0, newline);
        state.lineBuffer.erase(0, newline + 1);
        consumeLine(state, std::move(line));
        if (state.disconnected) return 0;
    }
    return bytes;
}

void runProvider(Json::Value input, ProviderConfig config, drogon::ResponseStreamPtr stream) {
    StreamState state{stream.get(), {}, 0, false, false};
    try {
        static const bool curlInitialized = [] {
            if (curl_global_init(CURL_GLOBAL_DEFAULT) != CURLE_OK)
                throw std::runtime_error("Could not initialize the Otto network client.");
            return true;
        }();
        (void)curlInitialized;
        const auto provider = input["provider"].asString();
        std::string apiKey;
        std::string model;
        std::string endpoint;
        std::string pinnedResolve;
        if (provider == "near_ai") {
            apiKey = config.nearApiKey;
            model = config.nearModel;
            endpoint = "https://cloud-api.near.ai/v1/chat/completions";
        } else if (provider == "qwen") {
            apiKey = config.groqApiKey;
            model = config.groqModel;
            endpoint = "https://api.groq.com/openai/v1/chat/completions";
        } else {
            apiKey = input["api_key"].asString();
            model = input["model"].asString();
            const auto target = byokTarget(input);
            endpoint = target.url;
            pinnedResolve = target.resolve;
        }
        if (apiKey.empty() || model.empty())
            throw std::runtime_error("This Otto provider is not configured. Choose another provider or contact support.");

        Json::Value payload;
        payload["model"] = model;
        payload["stream"] = true;
        payload["temperature"] = 0.35;
        payload["max_tokens"] = 900;
        Json::Value system;
        system["role"] = "system";
        system["content"] = "You are Otto, Afterbell's trading and market education assistant. Be concise, clear, and factual. The user selected " + input["market_id"].asString() + "-USDC; this is context only. No live market data has been supplied unless it appears in the conversation. Never invent prices, funding, balances, positions, or market events. Explain uncertainty and do not present yourself as financial advice. You cannot submit or execute trades.";
        payload["messages"].append(system);
        for (const auto &message : input["messages"]) {
            Json::Value item;
            item["role"] = message["role"];
            item["content"] = message["content"];
            payload["messages"].append(item);
        }

        Json::StreamWriterBuilder writer;
        writer["indentation"] = "";
        const auto body = Json::writeString(writer, payload);
        CurlPtr curl(curl_easy_init(), curl_easy_cleanup);
        if (!curl) throw std::runtime_error("Could not initialize the Otto connection.");
        HeaderPtr headers(nullptr, curl_slist_free_all);
        const auto appendHeader = [&headers](const char *value) {
            auto *updated = curl_slist_append(headers.get(), value);
            if (updated == nullptr) throw std::runtime_error("Could not prepare the Otto provider request.");
            headers.release();
            headers.reset(updated);
        };
        appendHeader("Content-Type: application/json");
        const auto auth = "Authorization: Bearer " + apiKey;
        appendHeader(auth.c_str());
        appendHeader("Accept: text/event-stream");
        HeaderPtr resolveList(nullptr, curl_slist_free_all);
        if (!pinnedResolve.empty()) {
            auto *updated = curl_slist_append(nullptr, pinnedResolve.c_str());
            if (updated == nullptr) throw std::runtime_error("Could not safely resolve the BYOK API host.");
            resolveList.reset(updated);
        }

        curl_easy_setopt(curl.get(), CURLOPT_URL, endpoint.c_str());
        curl_easy_setopt(curl.get(), CURLOPT_HTTPHEADER, headers.get());
        curl_easy_setopt(curl.get(), CURLOPT_POST, 1L);
        curl_easy_setopt(curl.get(), CURLOPT_POSTFIELDS, body.data());
        curl_easy_setopt(curl.get(), CURLOPT_POSTFIELDSIZE, static_cast<long>(body.size()));
        curl_easy_setopt(curl.get(), CURLOPT_HEADERFUNCTION, receiveHeaders);
        curl_easy_setopt(curl.get(), CURLOPT_HEADERDATA, &state);
        curl_easy_setopt(curl.get(), CURLOPT_WRITEFUNCTION, receiveBody);
        curl_easy_setopt(curl.get(), CURLOPT_WRITEDATA, &state);
        curl_easy_setopt(curl.get(), CURLOPT_CONNECTTIMEOUT, 10L);
        curl_easy_setopt(curl.get(), CURLOPT_TIMEOUT, 90L);
        curl_easy_setopt(curl.get(), CURLOPT_NOSIGNAL, 1L);
        curl_easy_setopt(curl.get(), CURLOPT_FOLLOWLOCATION, 0L);
        curl_easy_setopt(curl.get(), CURLOPT_PROTOCOLS_STR, "https");
        curl_easy_setopt(curl.get(), CURLOPT_USERAGENT, "Afterbell-Otto/1.0");
        if (resolveList) curl_easy_setopt(curl.get(), CURLOPT_RESOLVE, resolveList.get());

        const auto result = curl_easy_perform(curl.get());
        long status = 0;
        curl_easy_getinfo(curl.get(), CURLINFO_RESPONSE_CODE, &status);
        if (result != CURLE_OK) {
            if (!state.disconnected) {
                Json::Value error;
                error["message"] = result == CURLE_OPERATION_TIMEDOUT
                                       ? "Otto timed out. Please try again."
                                       : "Otto could not reach the selected provider. Please try again.";
                sendEvent(state, "error", error);
            }
        } else if (status < 200 || status >= 300) {
            Json::Value error;
            error["message"] = status == 429
                                   ? "The selected provider is busy or rate-limited. Please try again shortly."
                                   : "The selected provider rejected the request. Check its model and API key.";
            sendEvent(state, "error", error);
        } else if (!state.disconnected && !state.doneSent) {
            if (!state.lineBuffer.empty()) consumeLine(state, std::move(state.lineBuffer));
            if (!state.doneSent) {
                Json::Value done;
                done["finish"] = true;
                sendEvent(state, "done", done);
            }
        }
    } catch (const std::exception &error) {
        Json::Value message;
        message["message"] = std::string(error.what());
        sendEvent(state, "error", message);
    } catch (...) {
        Json::Value message;
        message["message"] = "Otto encountered an unexpected error. Please try again.";
        sendEvent(state, "error", message);
    }
    stream->close();
}

} // namespace

Json::Value providerStatus(const ProviderConfig &config) {
    Json::Value providers(Json::objectValue);
    providers["near_ai"]["available"] = !config.nearApiKey.empty() && !config.nearModel.empty();
    providers["near_ai"]["model"] = config.nearModel;
    providers["qwen"]["available"] = !config.groqApiKey.empty() && !config.groqModel.empty();
    providers["qwen"]["model"] = config.groqModel;
    providers["byok"]["available"] = true;
    return providers;
}

bool validChatRequest(const Json::Value &input, std::string &error) {
    if (!input.isObject() || !input["provider"].isString()) {
        error = "Choose an Otto provider.";
        return false;
    }
    const auto provider = input["provider"].asString();
    if (provider != "near_ai" && provider != "qwen" && provider != "byok") {
        error = "Unknown Otto provider.";
        return false;
    }
    if (!input["account_id"].isString() || input["account_id"].asString().empty() ||
        input["account_id"].asString().size() > 128) {
        error = "A connected wallet account is required to use Otto.";
        return false;
    }
    if (!input["market_id"].isString()) {
        error = "Choose a valid market pair.";
        return false;
    }
    const auto market = input["market_id"].asString();
    if (market != "AAPL" && market != "GOOGL" && market != "META" && market != "NVDA" && market != "TSLA") {
        error = "Choose a valid market pair.";
        return false;
    }
    if (!input["messages"].isArray() || input["messages"].empty() || input["messages"].size() > kMaxMessages) {
        error = "Send between one and sixteen messages.";
        return false;
    }
    std::size_t total = 0;
    for (const auto &message : input["messages"]) {
        if (!message.isObject() || !message["role"].isString() || !message["content"].isString()) {
            error = "Each message must include a role and text.";
            return false;
        }
        const auto role = message["role"].asString();
        const auto &content = message["content"].asString();
        if ((role != "user" && role != "assistant") || content.empty() || content.size() > kMaxMessageChars) {
            error = "Messages must be non-empty user or assistant text under 4,000 characters.";
            return false;
        }
        total += content.size();
    }
    if (total > kMaxTotalChars) {
        error = "This conversation is too long. Start a new chat with Otto.";
        return false;
    }
    if (provider == "byok") {
        if (!input["api_key"].isString() || input["api_key"].asString().empty() || input["api_key"].asString().size() > 512 ||
            !input["model"].isString() || input["model"].asString().empty() || input["model"].asString().size() > 160 ||
            !input["base_url"].isString() || input["base_url"].asString().size() > 512) {
            error = "Enter a valid compatible API URL, model, and API key.";
            return false;
        }
    }
    return true;
}

bool allowRequestForAccount(const std::string &accountId) {
    return accountRequestAllowed(accountId);
}

drogon::HttpResponsePtr streamChat(Json::Value input, ProviderConfig config) {
    auto response = drogon::HttpResponse::newAsyncStreamResponse(
        [input = std::move(input), config = std::move(config)](drogon::ResponseStreamPtr stream) mutable {
            std::thread(runProvider, std::move(input), std::move(config), std::move(stream)).detach();
        }, true);
    response->setContentTypeCodeAndCustomString(drogon::CT_CUSTOM, "text/event-stream; charset=utf-8");
    response->addHeader("Cache-Control", "no-cache, no-transform");
    response->addHeader("X-Accel-Buffering", "no");
    return response;
}

} // namespace afterbell::ai
