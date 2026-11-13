#include "ai/otto.hpp"

#include <curl/curl.h>
#include <openssl/evp.h>
#include <openssl/rand.h>
#include "oracle/crypto.hpp"

#include <arpa/inet.h>
#include <netdb.h>
#include <sys/socket.h>

#include <algorithm>
#include <chrono>
#include <cctype>
#include <deque>
#include <future>
#include <iostream>
#include <memory>
#include <iomanip>
#include <limits>
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
    std::string errorBody;
    std::string upstreamRequestId;
    long status = 0;
    bool disconnected = false;
    bool doneSent = false;
};

std::mutex rateMutex;
std::unordered_map<std::string, std::deque<std::chrono::steady_clock::time_point>> recentRequests;

struct WalletChallenge {
    std::string accountId;
    std::string message;
    std::string nonce;
    std::string recipient;
    Json::Value owner;
    std::chrono::system_clock::time_point expires;
};
std::mutex sessionMutex;
std::unordered_map<std::string, WalletChallenge> challenges;
std::unordered_map<std::string, std::pair<std::string, std::chrono::system_clock::time_point>> sessions;

std::vector<std::uint8_t> secureRandom(std::size_t size) {
    std::vector<std::uint8_t> bytes(size);
    if (size > static_cast<std::size_t>(std::numeric_limits<int>::max()) || RAND_bytes(bytes.data(), static_cast<int>(size)) != 1)
        throw std::runtime_error("Secure wallet session could not be initialized.");
    return bytes;
}
std::string hexBytes(const std::vector<std::uint8_t> &bytes) {
    std::ostringstream out;
    out << std::hex << std::setfill('0');
    for (auto b : bytes) out << std::setw(2) << static_cast<unsigned>(b);
    return out.str();
}
std::vector<std::uint8_t> ownerBytes(const Json::Value &owner, const std::string &variant, const std::string &field) {
    const auto &values = owner[variant][field];
    if (!values.isArray()) throw std::invalid_argument("Wallet owner key is invalid.");
    std::vector<std::uint8_t> bytes;
    for (const auto &value : values) {
        if (!value.isUInt() || value.asUInt() > 255) throw std::invalid_argument("Wallet owner key is invalid.");
        bytes.push_back(static_cast<std::uint8_t>(value.asUInt()));
    }
    return bytes;
}
void appendLe32(std::vector<std::uint8_t> &out, std::uint32_t value) {
    for (unsigned i = 0; i < 4; ++i) out.push_back(static_cast<std::uint8_t>(value >> (8 * i)));
}
void appendBorshString(std::vector<std::uint8_t> &out, const std::string &value) {
    if (value.size() > std::numeric_limits<std::uint32_t>::max()) throw std::invalid_argument("Invalid wallet signature message.");
    appendLe32(out, static_cast<std::uint32_t>(value.size()));
    out.insert(out.end(), value.begin(), value.end());
}
bool verifyEd25519(const std::vector<std::uint8_t> &key, const std::vector<std::uint8_t> &message,
                   const std::vector<std::uint8_t> &signature) {
    if (key.size() != 32 || signature.size() != 64) return false;
    EVP_PKEY *raw = EVP_PKEY_new_raw_public_key(EVP_PKEY_ED25519, nullptr, key.data(), key.size());
    if (!raw) return false;
    std::unique_ptr<EVP_PKEY, decltype(&EVP_PKEY_free)> pkey(raw, EVP_PKEY_free);
    EVP_MD_CTX *rawCtx = EVP_MD_CTX_new();
    if (!rawCtx) return false;
    std::unique_ptr<EVP_MD_CTX, decltype(&EVP_MD_CTX_free)> ctx(rawCtx, EVP_MD_CTX_free);
    return EVP_DigestVerifyInit(ctx.get(), nullptr, nullptr, nullptr, pkey.get()) == 1 &&
           EVP_DigestVerify(ctx.get(), signature.data(), signature.size(), message.data(), message.size()) == 1;
}
bool verifyWalletSignature(const WalletChallenge &challenge, const std::vector<std::uint8_t> &signature) {
    const auto &owner = challenge.owner;
    if (challenge.accountId.rfind("solana:", 0) == 0) {
        const auto key = ownerBytes(owner, "SolanaEd25519", "public_key");
        return verifyEd25519(key, std::vector<std::uint8_t>(challenge.message.begin(), challenge.message.end()), signature);
    }
    if (challenge.accountId.rfind("ethereum:", 0) == 0) {
        const auto address = ownerBytes(owner, "Ethereum", "address");
        return afterbell::oracle::eip191Verify(challenge.message, signature, address);
    }
    if (challenge.accountId.rfind("near:", 0) == 0) {
        const auto key = ownerBytes(owner, "NearEd25519", "public_key");
        auto nonce = afterbell::oracle::base64Decode(challenge.nonce);
        if (nonce.size() != 32) return false;
        std::vector<std::uint8_t> borsh;
        appendLe32(borsh, 2147484061U); // NEP-413 tag: 2^31 + 413
        appendBorshString(borsh, challenge.message);
        borsh.insert(borsh.end(), nonce.begin(), nonce.end());
        appendBorshString(borsh, challenge.recipient);
        borsh.push_back(0); // callbackUrl: None
        const auto hash = afterbell::oracle::sha256(borsh.data(), borsh.size());
        return verifyEd25519(key, std::vector<std::uint8_t>(hash.begin(), hash.end()), signature);
    }
    return false;
}

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
    if (line.rfind("HTTP/", 0) == 0) {
        consumeLine(state, std::move(line));
    } else {
        constexpr std::string_view requestIdHeader = "x-request-id:";
        std::string lower = line;
        std::transform(lower.begin(), lower.end(), lower.begin(), [](unsigned char c) {
            return static_cast<char>(std::tolower(c));
        });
        if (lower.rfind(requestIdHeader, 0) == 0) {
            auto value = line.substr(requestIdHeader.size());
            while (!value.empty() && std::isspace(static_cast<unsigned char>(value.front()))) value.erase(value.begin());
            while (!value.empty() && std::isspace(static_cast<unsigned char>(value.back()))) value.pop_back();
            if (value.size() <= 128 && value.find_first_of("\r\n") == std::string::npos)
                state.upstreamRequestId = std::move(value);
        }
    }
    return bytes;
}

std::size_t receiveBody(char *data, std::size_t size, std::size_t count, void *opaque) {
    const auto bytes = size * count;
    auto &state = *static_cast<StreamState *>(opaque);
    if (state.status < 200 || state.status >= 300) {
        constexpr std::size_t maxErrorBodyBytes = 8192;
        const auto remaining = maxErrorBodyBytes - std::min(maxErrorBodyBytes, state.errorBody.size());
        state.errorBody.append(data, std::min(bytes, remaining));
        return bytes;
    }
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

std::string safeProviderErrorDetail(const std::string &body, const std::string &apiKey) {
    Json::Value parsed;
    std::istringstream input(body);
    std::string parseError;
    if (!Json::parseFromStream(Json::CharReaderBuilder{}, input, &parsed, &parseError)) return {};
    auto message = parsed["error"]["message"].asString();
    if (message.empty()) return {};
    if (!apiKey.empty()) {
        std::size_t position = 0;
        while ((position = message.find(apiKey, position)) != std::string::npos) {
            message.replace(position, apiKey.size(), "[redacted]");
            position += 10;
        }
    }
    message.erase(std::remove_if(message.begin(), message.end(), [](unsigned char c) {
        return std::iscntrl(c) && !std::isspace(c);
    }), message.end());
    if (message.size() > 300) message.resize(300);
    return message;
}

std::string providerRejectionMessage(const std::string &provider, const std::string &model,
                                     long status, const std::string &body, const std::string &apiKey) {
    const auto name = provider == "near_ai" ? "NEAR AI" : provider == "qwen" ? "Qwen/Groq" : "BYOK provider";
    if (status == 401 || status == 403)
        return std::string(name) + " rejected the API key or denied access to this model. Check the key and model permissions.";
    if (status == 402)
        return std::string(name) + " has no available credits for this request. Check its billing balance.";
    if (provider == "near_ai" && status == 404) {
        if (model == "openai/privacy-filter")
            return "NEAR_AI_MODEL is set to openai/privacy-filter, which is only for privacy redaction. Set it to a chat model such as zai-org/GLM-5.1-FP8.";
        return "NEAR AI did not find the configured chat model '" + model + "'. Check NEAR_AI_MODEL against the available model IDs.";
    }
    if (status == 429) return std::string(name) + " is busy or rate-limited. Please try again shortly.";
    const auto detail = safeProviderErrorDetail(body, apiKey);
    if (!detail.empty()) return std::string(name) + " rejected the request: " + detail;
    return std::string(name) + " rejected the request (HTTP " + std::to_string(status) + "). Check its model and API key.";
}

std::size_t collectBody(char *data, std::size_t size, std::size_t count, void *opaque) {
    const auto bytes = size * count;
    auto *body = static_cast<std::string *>(opaque);
    if (body->size() + bytes > 128 * 1024) return 0;
    body->append(data, bytes);
    return bytes;
}

void redactNearMessages(Json::Value &messages, const std::string &apiKey) {
    Json::Value inputs(Json::arrayValue);
    std::vector<Json::ArrayIndex> userMessageIndexes;
    for (Json::ArrayIndex i = 0; i < messages.size(); ++i) {
        if (messages[i]["role"].asString() != "user") continue;
        inputs.append(messages[i]["content"]);
        userMessageIndexes.push_back(i);
    }
    Json::Value payload;
    payload["model"] = "openai/privacy-filter";
    payload["input"] = inputs;
    Json::StreamWriterBuilder writer;
    writer["indentation"] = "";
    const auto body = Json::writeString(writer, payload);
    CurlPtr curl(curl_easy_init(), curl_easy_cleanup);
    if (!curl) throw std::runtime_error("NEAR AI privacy filtering is unavailable. Your message was not sent.");
    HeaderPtr headers(nullptr, curl_slist_free_all);
    headers.reset(curl_slist_append(nullptr, "Content-Type: application/json"));
    const auto auth = "Authorization: Bearer " + apiKey;
    headers.reset(curl_slist_append(headers.release(), auth.c_str()));
    std::string responseBody;
    curl_easy_setopt(curl.get(), CURLOPT_URL, "https://cloud-api.near.ai/v1/privacy/redact");
    curl_easy_setopt(curl.get(), CURLOPT_HTTPHEADER, headers.get());
    curl_easy_setopt(curl.get(), CURLOPT_POST, 1L);
    curl_easy_setopt(curl.get(), CURLOPT_POSTFIELDS, body.data());
    curl_easy_setopt(curl.get(), CURLOPT_POSTFIELDSIZE, static_cast<long>(body.size()));
    curl_easy_setopt(curl.get(), CURLOPT_WRITEFUNCTION, collectBody);
    curl_easy_setopt(curl.get(), CURLOPT_WRITEDATA, &responseBody);
    curl_easy_setopt(curl.get(), CURLOPT_CONNECTTIMEOUT, 8L);
    curl_easy_setopt(curl.get(), CURLOPT_TIMEOUT, 20L);
    curl_easy_setopt(curl.get(), CURLOPT_NOSIGNAL, 1L);
    curl_easy_setopt(curl.get(), CURLOPT_FOLLOWLOCATION, 0L);
    curl_easy_setopt(curl.get(), CURLOPT_PROTOCOLS_STR, "https");
    const auto result = curl_easy_perform(curl.get());
    long status = 0;
    curl_easy_getinfo(curl.get(), CURLINFO_RESPONSE_CODE, &status);
    if (result != CURLE_OK || status < 200 || status >= 300)
        throw std::runtime_error("NEAR AI privacy filtering is unavailable. Your message was not sent.");
    Json::Value redacted;
    std::istringstream stream(responseBody);
    std::string errors;
    if (!Json::parseFromStream(Json::CharReaderBuilder{}, stream, &redacted, &errors) ||
        !redacted["data"].isArray() || redacted["data"].size() != userMessageIndexes.size())
        throw std::runtime_error("NEAR AI privacy filtering returned an invalid response. Your message was not sent.");
    for (Json::ArrayIndex i = 0; i < userMessageIndexes.size(); ++i) {
        if (!redacted["data"][i]["redacted"].isString())
            throw std::runtime_error("NEAR AI privacy filtering returned an invalid response. Your message was not sent.");
        messages[userMessageIndexes[i]]["content"] = redacted["data"][i]["redacted"];
    }
}

void runProvider(Json::Value input, ProviderConfig config, std::function<Json::Value()> contextProvider, drogon::ResponseStreamPtr stream) {
    StreamState state;
    state.stream = stream.get();
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
        Json::Value context(Json::objectValue);
        if (contextProvider) {
            Json::Value status; status["message"] = "Fetching your latest market and account data…";
            sendEvent(state, "status", status);
            try { context = contextProvider(); }
            catch (...) { context["context_error"] = "Some live account context could not be loaded."; }
        }
        Json::StreamWriterBuilder contextWriter;
        contextWriter["indentation"] = "";
        Json::Value system;
        system["role"] = "system";
        system["content"] = "You are Otto, Afterbell's trading and market education assistant. Be concise, clear, and factual. The user selected " + input["market_id"].asString() + "-USDC. The following trusted, server-fetched live account and market context was loaded for this request: " + Json::writeString(contextWriter, context) + ". Treat timestamps as milliseconds since Unix epoch. Prices are E8 integers; USDC values are raw 6-decimal integers unless explicitly serialized as decimal USDC strings. State the observed time when freshness matters, never invent missing values or claim stale data is current, and distinguish current snapshots from historical price action. Explain uncertainty and do not present yourself as financial advice. You cannot submit or execute trades.";
        payload["messages"].append(system);
        for (const auto &message : input["messages"]) {
            Json::Value item;
            item["role"] = message["role"];
            item["content"] = message["content"];
            payload["messages"].append(item);
        }
        if (provider == "near_ai") {
            Json::Value status; status["message"] = "Applying NEAR AI privacy filtering…";
            sendEvent(state, "status", status);
            redactNearMessages(payload["messages"], apiKey);
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
            std::clog << "otto.provider_rejected provider=" << provider << " model=" << model
                      << " status=" << status << " request_id=" << state.upstreamRequestId << '\n';
            Json::Value error;
            error["message"] = providerRejectionMessage(provider, model, status, state.errorBody, apiKey);
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
    const bool nearModelConfigured = !config.nearModel.empty() && config.nearModel != "openai/privacy-filter";
    providers["near_ai"]["available"] = !config.nearApiKey.empty() && nearModelConfigured;
    providers["near_ai"]["model"] = config.nearModel;
    if (!nearModelConfigured)
        providers["near_ai"]["message"] = "Set NEAR_AI_MODEL to a chat model. openai/privacy-filter is reserved for privacy redaction.";
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

Json::Value createWalletChallenge(const std::string &accountId, const Json::Value &owner, const std::string &recipient) {
    if (recipient.empty() || (accountId.rfind("near:", 0) != 0 && accountId.rfind("solana:", 0) != 0 && accountId.rfind("ethereum:", 0) != 0))
        throw std::invalid_argument("Connect a supported registered wallet to use Otto.");
    const auto randomId = secureRandom(24);
    const auto nonceBytes = secureRandom(32);
    const auto nonce = afterbell::oracle::base64Encode(nonceBytes.data(), nonceBytes.size());
    const auto challengeId = hexBytes(randomId);
    const auto expires = std::chrono::system_clock::now() + std::chrono::minutes(5);
    const auto expiresMs = std::chrono::duration_cast<std::chrono::milliseconds>(expires.time_since_epoch()).count();
    WalletChallenge challenge;
    challenge.accountId = accountId;
    challenge.nonce = nonce;
    challenge.recipient = recipient;
    challenge.owner = owner;
    challenge.expires = expires;
    challenge.message = "Afterbell Otto wallet session\nAccount: " + accountId + "\nDomain: " + recipient + "\nNonce: " + hexBytes(nonceBytes) + "\nExpires: " + std::to_string(expiresMs) + "\nThis signature only creates a temporary Otto chat session. It cannot move funds or submit transactions.";
    {
        std::lock_guard<std::mutex> lock(sessionMutex);
        for (auto it = challenges.begin(); it != challenges.end();) {
            if (it->second.expires < std::chrono::system_clock::now()) it = challenges.erase(it);
            else ++it;
        }
        challenges[challengeId] = challenge;
    }
    Json::Value payload;
    payload["message"] = challenge.message;
    payload["nonce"] = nonce;
    payload["recipient"] = recipient;
    Json::Value result;
    result["challenge_id"] = challengeId;
    result["payload"] = payload;
    result["expires_at_ms"] = Json::Int64(expiresMs);
    return result;
}

std::string walletChallengeAccount(const std::string &challengeId) {
    std::lock_guard<std::mutex> lock(sessionMutex);
    const auto found = challenges.find(challengeId);
    return found == challenges.end() || found->second.expires < std::chrono::system_clock::now() ? std::string{} : found->second.accountId;
}

std::string verifyWalletChallenge(const std::string &challengeId, const std::string &signatureBase64, const Json::Value &currentOwner) {
    WalletChallenge challenge;
    {
        std::lock_guard<std::mutex> lock(sessionMutex);
        const auto found = challenges.find(challengeId);
        if (found == challenges.end()) throw std::invalid_argument("Wallet session request expired. Please try again.");
        challenge = found->second;
        challenges.erase(found); // every challenge can only be attempted once
    }
    if (challenge.expires < std::chrono::system_clock::now()) throw std::invalid_argument("Wallet session request expired. Please try again.");
    Json::StreamWriterBuilder writer;
    writer["indentation"] = "";
    if (Json::writeString(writer, challenge.owner) != Json::writeString(writer, currentOwner))
        throw std::invalid_argument("The connected wallet no longer matches this Afterbell account.");
    std::vector<std::uint8_t> signature;
    try { signature = afterbell::oracle::base64Decode(signatureBase64); }
    catch (...) { throw std::invalid_argument("Wallet signature was invalid."); }
    if (!verifyWalletSignature(challenge, signature)) throw std::invalid_argument("Wallet signature could not be verified.");
    const auto token = hexBytes(secureRandom(32));
    {
        std::lock_guard<std::mutex> lock(sessionMutex);
        for (auto it = sessions.begin(); it != sessions.end();) {
            if (it->second.second < std::chrono::system_clock::now()) it = sessions.erase(it);
            else ++it;
        }
        sessions[token] = {challenge.accountId, std::chrono::system_clock::now() + std::chrono::minutes(30)};
    }
    return token;
}

std::string walletSessionAccount(const std::string &token) {
    if (token.empty() || token.size() > 128) return {};
    std::lock_guard<std::mutex> lock(sessionMutex);
    const auto found = sessions.find(token);
    if (found == sessions.end()) return {};
    if (found->second.second < std::chrono::system_clock::now()) {
        sessions.erase(found);
        return {};
    }
    return found->second.first;
}

drogon::HttpResponsePtr streamChat(Json::Value input, ProviderConfig config, std::function<Json::Value()> contextProvider) {
    auto response = drogon::HttpResponse::newAsyncStreamResponse(
        [input = std::move(input), config = std::move(config), contextProvider = std::move(contextProvider)](drogon::ResponseStreamPtr stream) mutable {
            std::thread(runProvider, std::move(input), std::move(config), std::move(contextProvider), std::move(stream)).detach();
        }, true);
    response->setContentTypeCodeAndCustomString(drogon::CT_CUSTOM, "text/event-stream; charset=utf-8");
    response->addHeader("Cache-Control", "no-cache, no-transform");
    response->addHeader("X-Accel-Buffering", "no");
    return response;
}

} // namespace afterbell::ai
