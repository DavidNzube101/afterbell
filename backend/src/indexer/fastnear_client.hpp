#pragma once

#include <json/json.h>

#include <cstdint>
#include <string>
#include <vector>

namespace afterbell::indexer {

struct AccountTransaction {
    std::string hash;
    std::uint64_t blockHeight{};
};

class FastNearClient {
  public:
    explicit FastNearClient(std::string baseUrl);

    std::vector<AccountTransaction> accountTransactions(
        const std::string &accountId,
        std::uint64_t fromBlock,
        std::uint64_t toBlock);
    std::vector<Json::Value> transactions(const std::vector<std::string> &hashes);

  private:
    std::string baseUrl_;
    std::string authorizationHeader_;
    Json::Value post(const std::string &path, const Json::Value &body);
};

} // namespace afterbell::indexer
