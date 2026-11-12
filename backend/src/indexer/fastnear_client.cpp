#include "indexer/fastnear_client.hpp"
#include "indexer/fastnear_auth.hpp"

#include <curl/curl.h>

#include <algorithm>
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

std::uint64_t unsignedField(const Json::Value &object, const char *field) {
    const auto &value = object[field];
    if (value.isUInt64()) return value.asUInt64();
    if (value.isInt64() && value.asInt64() >= 0) {
        return static_cast<std::uint64_t>(value.asInt64());
    }
    if (value.isString()) return std::stoull(value.asString());
    throw std::runtime_error(std::string("FastNear omitted numeric field ") + field);
}

std::string stringField(const Json::Value &object, const char *field) {
    const auto &value = object[field];
    if (!value.isString()) {
        throw std::runtime_error(std::string("FastNear omitted string field ") + field);
    }
    return value.asString();
}

} // namespace

FastNearClient::FastNearClient(std::string baseUrl)
    : baseUrl_(std::move(baseUrl)),
      authorizationHeader_(fastNearAuthorizationHeader(baseUrl_)) {
    (void)curlGlobal();
    while (!baseUrl_.empty() && baseUrl_.back() == '/') baseUrl_.pop_back();
    if (baseUrl_.empty()) throw std::invalid_argument("INDEXER_TX_API_URL is required");
}

Json::Value FastNearClient::post(const std::string &path, const Json::Value &body) {
    std::unique_ptr<CURL, decltype(&curl_easy_cleanup)> client(curl_easy_init(),
                                                              curl_easy_cleanup);
    if (!client) throw std::runtime_error("Could not allocate HTTP client");
    const auto requestBody = compactJson(body);
    std::string responseBody;
    struct curl_slist *headers = nullptr;
    headers = curl_slist_append(headers, "Content-Type: application/json");
    headers = curl_slist_append(headers, "Accept: application/json");
    if (!authorizationHeader_.empty()) {
        headers = curl_slist_append(headers, authorizationHeader_.c_str());
    }
    std::unique_ptr<curl_slist, decltype(&curl_slist_free_all)> headerGuard(
        headers, curl_slist_free_all);

    const auto url = baseUrl_ + path;
    curl_easy_setopt(client.get(), CURLOPT_URL, url.c_str());
    curl_easy_setopt(client.get(), CURLOPT_HTTPHEADER, headerGuard.get());
    curl_easy_setopt(client.get(), CURLOPT_POST, 1L);
    curl_easy_setopt(client.get(), CURLOPT_POSTFIELDS, requestBody.data());
    curl_easy_setopt(client.get(), CURLOPT_POSTFIELDSIZE,
                     static_cast<long>(requestBody.size()));
    curl_easy_setopt(client.get(), CURLOPT_WRITEFUNCTION, receiveBody);
    curl_easy_setopt(client.get(), CURLOPT_WRITEDATA, &responseBody);
    curl_easy_setopt(client.get(), CURLOPT_CONNECTTIMEOUT, 10L);
    curl_easy_setopt(client.get(), CURLOPT_TIMEOUT, 45L);
    curl_easy_setopt(client.get(), CURLOPT_NOSIGNAL, 1L);
    curl_easy_setopt(client.get(), CURLOPT_FOLLOWLOCATION, 0L);
    curl_easy_setopt(client.get(), CURLOPT_USERAGENT, "Afterbell-Indexer/0.2");

    const auto result = curl_easy_perform(client.get());
    if (result != CURLE_OK) {
        throw std::runtime_error("FastNear transport failed: " +
                                 std::string(curl_easy_strerror(result)));
    }
    long status = 0;
    curl_easy_getinfo(client.get(), CURLINFO_RESPONSE_CODE, &status);
    if (status < 200 || status >= 300) {
        throw std::runtime_error("FastNear returned HTTP " + std::to_string(status));
    }

    Json::CharReaderBuilder builder;
    Json::Value response;
    std::string errors;
    const auto reader = std::unique_ptr<Json::CharReader>(builder.newCharReader());
    if (!reader->parse(responseBody.data(), responseBody.data() + responseBody.size(),
                       &response, &errors) || !response.isObject()) {
        throw std::runtime_error("FastNear returned invalid JSON");
    }
    return response;
}

std::vector<AccountTransaction> FastNearClient::accountTransactions(
    const std::string &accountId,
    std::uint64_t fromBlock,
    std::uint64_t toBlock) {
    std::vector<AccountTransaction> transactions;
    std::string resumeToken;
    do {
        Json::Value request;
        request["account_id"] = accountId;
        request["is_receiver"] = true;
        request["from_tx_block_height"] = Json::UInt64(fromBlock);
        request["to_tx_block_height"] = Json::UInt64(toBlock);
        request["desc"] = false;
        request["limit"] = 200;
        if (!resumeToken.empty()) request["resume_token"] = resumeToken;

        const auto response = post("/v0/account", request);
        const auto &rows = response["account_txs"];
        if (!rows.isArray()) {
            throw std::runtime_error("FastNear account response omitted account_txs");
        }
        for (const auto &row : rows) {
            const auto hash = stringField(row, "transaction_hash");
            const auto blockHeight = unsignedField(row, "tx_block_height");
            if (blockHeight >= fromBlock && blockHeight <= toBlock) {
                transactions.push_back({hash, blockHeight});
            }
        }

        const auto &next = response["resume_token"];
        const auto nextToken = next.isString() ? next.asString() : std::string{};
        if (!nextToken.empty() && nextToken == resumeToken) {
            throw std::runtime_error("FastNear pagination cursor did not advance");
        }
        resumeToken = nextToken;
    } while (!resumeToken.empty());

    std::sort(transactions.begin(), transactions.end(),
              [](const auto &left, const auto &right) {
                  if (left.blockHeight != right.blockHeight) {
                      return left.blockHeight < right.blockHeight;
                  }
                  return left.hash < right.hash;
              });
    transactions.erase(
        std::unique(transactions.begin(), transactions.end(),
                    [](const auto &left, const auto &right) {
                        return left.hash == right.hash;
                    }),
        transactions.end());
    return transactions;
}

std::vector<Json::Value> FastNearClient::transactions(
    const std::vector<std::string> &hashes) {
    std::vector<Json::Value> result;
    result.reserve(hashes.size());
    for (std::size_t offset = 0; offset < hashes.size(); offset += 20) {
        Json::Value request;
        request["tx_hashes"] = Json::Value(Json::arrayValue);
        const auto end = std::min(hashes.size(), offset + 20);
        for (auto index = offset; index < end; ++index) {
            request["tx_hashes"].append(hashes[index]);
        }
        const auto response = post("/v0/transactions", request);
        const auto &rows = response["transactions"];
        if (!rows.isArray() || rows.size() != end - offset) {
            throw std::runtime_error("FastNear returned an incomplete transaction batch");
        }
        for (const auto &row : rows) result.push_back(row);
    }
    return result;
}

} // namespace afterbell::indexer
