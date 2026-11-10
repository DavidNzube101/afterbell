#include "indexer/near_rpc.hpp"

#include <curl/curl.h>

#include <memory>
#include <stdexcept>
#include <utility>

namespace afterbell::indexer {
namespace {

struct CurlGlobal {
    CurlGlobal() {
        if (curl_global_init(CURL_GLOBAL_DEFAULT) != CURLE_OK) {
            throw std::runtime_error("Could not initialize HTTP client");
        }
    }
    ~CurlGlobal() { curl_global_cleanup(); }
};

CurlGlobal &curlGlobal() {
    static CurlGlobal global;
    return global;
}

std::size_t receiveBody(char *data, std::size_t size, std::size_t count,
                        void *context) {
    const auto bytes = size * count;
    static_cast<std::string *>(context)->append(data, bytes);
    return bytes;
}

std::string compactJson(const Json::Value &value) {
    Json::StreamWriterBuilder builder;
    builder["indentation"] = "";
    return Json::writeString(builder, value);
}

} // namespace

NearRpc::NearRpc(std::string endpoint) : endpoint_(std::move(endpoint)) {
    (void)curlGlobal();
    if (endpoint_.empty()) throw std::invalid_argument("NEAR_RPC_URL is required");
}

NearRpc::~NearRpc() = default;

Json::Value NearRpc::call(const std::string &method, const Json::Value &params) {
    Json::Value request;
    request["jsonrpc"] = "2.0";
    request["id"] = Json::UInt64(++requestId_);
    request["method"] = method;
    request["params"] = params;
    const auto body = compactJson(request);

    std::unique_ptr<CURL, decltype(&curl_easy_cleanup)> client(curl_easy_init(),
                                                              curl_easy_cleanup);
    if (!client) throw std::runtime_error("Could not allocate HTTP client");
    std::string responseBody;
    struct curl_slist *headers = nullptr;
    headers = curl_slist_append(headers, "Content-Type: application/json");
    std::unique_ptr<curl_slist, decltype(&curl_slist_free_all)> headerGuard(
        headers, curl_slist_free_all);

    curl_easy_setopt(client.get(), CURLOPT_URL, endpoint_.c_str());
    curl_easy_setopt(client.get(), CURLOPT_HTTPHEADER, headerGuard.get());
    curl_easy_setopt(client.get(), CURLOPT_POST, 1L);
    curl_easy_setopt(client.get(), CURLOPT_POSTFIELDS, body.data());
    curl_easy_setopt(client.get(), CURLOPT_POSTFIELDSIZE, static_cast<long>(body.size()));
    curl_easy_setopt(client.get(), CURLOPT_WRITEFUNCTION, receiveBody);
    curl_easy_setopt(client.get(), CURLOPT_WRITEDATA, &responseBody);
    curl_easy_setopt(client.get(), CURLOPT_CONNECTTIMEOUT, 10L);
    curl_easy_setopt(client.get(), CURLOPT_TIMEOUT, 45L);
    curl_easy_setopt(client.get(), CURLOPT_NOSIGNAL, 1L);
    curl_easy_setopt(client.get(), CURLOPT_FOLLOWLOCATION, 0L);
    curl_easy_setopt(client.get(), CURLOPT_USERAGENT, "Afterbell-Indexer/0.1");

    const auto result = curl_easy_perform(client.get());
    if (result != CURLE_OK) {
        // Never include endpoint_ in errors; RPC URLs may contain credentials.
        throw std::runtime_error("NEAR RPC transport failed for method " + method +
                                 ": " + curl_easy_strerror(result));
    }
    long status = 0;
    curl_easy_getinfo(client.get(), CURLINFO_RESPONSE_CODE, &status);
    if (status < 200 || status >= 300) {
        std::string detail;
        Json::CharReaderBuilder errorBuilder;
        Json::Value errorResponse;
        std::string parseErrors;
        const auto errorReader = std::unique_ptr<Json::CharReader>(errorBuilder.newCharReader());
        if (errorReader->parse(responseBody.data(), responseBody.data() + responseBody.size(),
                                &errorResponse, &parseErrors) && errorResponse["error"].isObject()) {
            const auto &rpcError = errorResponse["error"];
            if (rpcError["code"].isNumeric()) {
                detail = " (code " + std::to_string(rpcError["code"].asInt64());
                if (rpcError["message"].isString()) detail += ": " + rpcError["message"].asString();
                detail += ")";
            } else if (rpcError["message"].isString()) {
                detail = " (" + rpcError["message"].asString() + ")";
            }
            constexpr std::size_t maxDetailLength = 320;
            if (detail.size() > maxDetailLength) detail.resize(maxDetailLength);
        }
        throw std::runtime_error("NEAR RPC returned HTTP " + std::to_string(status) +
                                 " for method " + method + detail);
    }

    Json::CharReaderBuilder builder;
    Json::Value response;
    std::string errors;
    const auto reader = std::unique_ptr<Json::CharReader>(builder.newCharReader());
    if (!reader->parse(responseBody.data(), responseBody.data() + responseBody.size(),
                       &response, &errors)) {
        throw std::runtime_error("NEAR RPC returned invalid JSON for method " + method);
    }
    if (response.isMember("error")) {
        const auto &rpcError = response["error"];
        const auto &code = rpcError["code"];
        const auto codeText = code.isString() ? code.asString()
                                              : code.isNumeric() ? std::to_string(code.asInt64())
                                                                 : std::string{};
        std::string detail;
        if (rpcError["message"].isString()) detail = rpcError["message"].asString();
        const auto &data = rpcError["data"];
        if (data.isString() && !data.asString().empty()) {
            if (!detail.empty()) detail += ": ";
            detail += data.asString();
        } else if (data.isObject()) {
            for (const auto *field : {"error_message", "message"}) {
                if (data[field].isString() && !data[field].asString().empty()) {
                    if (!detail.empty()) detail += ": ";
                    detail += data[field].asString();
                    break;
                }
            }
        }
        constexpr std::size_t maxDetailLength = 320;
        if (detail.size() > maxDetailLength) detail.resize(maxDetailLength);
        throw std::runtime_error("NEAR RPC returned an error for method " + method +
                                 (codeText.empty() ? std::string{} : " (code " + codeText + ")") +
                                 (detail.empty() ? std::string{} : ": " + detail));
    }
    if (!response.isMember("result")) {
        throw std::runtime_error("NEAR RPC response omitted result for method " + method);
    }
    return response["result"];
}

Json::Value NearRpc::finalizedHead() {
    Json::Value params;
    params["finality"] = "final";
    return call("block", params);
}

std::string NearRpc::chainId() {
    const auto status = call("status", Json::Value(Json::arrayValue));
    if (!status["chain_id"].isString()) {
        throw std::runtime_error("NEAR RPC status response omitted chain_id");
    }
    return status["chain_id"].asString();
}

Json::Value NearRpc::block(std::uint64_t height) {
    Json::Value params;
    params["block_id"] = Json::UInt64(height);
    return call("block", params);
}

Json::Value NearRpc::chunk(const std::string &chunkHash) {
    Json::Value params;
    params["chunk_id"] = chunkHash;
    return call("chunk", params);
}

Json::Value NearRpc::transactionStatus(const std::string &hash,
                                       const std::string &signerId) {
    Json::Value params;
    params["tx_hash"] = hash;
    params["sender_account_id"] = signerId;
    params["wait_until"] = "FINAL";
    return call("tx", params);
}

} // namespace afterbell::indexer
