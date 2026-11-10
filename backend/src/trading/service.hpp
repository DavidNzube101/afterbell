#pragma once

#include <json/json.h>

#include <cstdint>
#include <memory>
#include <string>

namespace afterbell::trading {

struct Config {
    std::string network;
    std::string readRpcUrl;
    std::string transactionRpcUrl;
    std::string contractId;
    std::string relayerAccountId;
    std::string relayerPrivateKey;
    std::string quoteSignerPrivateKey;
    std::uint16_t feeBps;
};

class Service {
  public:
    explicit Service(Config config);
    Json::Value createOpenQuote(const Json::Value &request) const;
    Json::Value submitOpen(const Json::Value &request);

  private:
    Config config_;
};

} // namespace afterbell::trading
