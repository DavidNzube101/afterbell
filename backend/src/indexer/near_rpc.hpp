#pragma once

#include <json/json.h>

#include <cstdint>
#include <string>

namespace afterbell::indexer {

class NearRpc {
  public:
    explicit NearRpc(std::string endpoint);
    ~NearRpc();

    NearRpc(const NearRpc &) = delete;
    NearRpc &operator=(const NearRpc &) = delete;

    Json::Value call(const std::string &method, const Json::Value &params);
    Json::Value finalizedHead();
    std::string chainId();
    Json::Value block(std::uint64_t height);
    Json::Value chunk(const std::string &chunkHash);
    Json::Value transactionStatus(const std::string &hash,
                                 const std::string &signerId);

  private:
    std::string endpoint_;
    std::string authorizationHeader_;
    std::uint64_t requestId_{0};
};

} // namespace afterbell::indexer
