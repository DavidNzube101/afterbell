#pragma once

#include <json/json.h>

#include <cstdint>
#include <memory>
#include <atomic>
#include <string>
#include <thread>

namespace afterbell::trading {

struct Config {
    std::string network;
    std::string readRpcUrl;
    std::string transactionRpcUrl;
    std::string contractId;
    std::string relayerAccountId;
    std::string relayerPrivateKey;
    std::string quoteSignerPrivateKey;
    std::string ownerRegistrationDepositYocto;
    std::uint16_t feeBps;
};

class Service {
  public:
    explicit Service(Config config);
    ~Service();
    Json::Value createOpenQuote(const Json::Value &request) const;
    Json::Value submitOpen(const Json::Value &request);
    Json::Value listPositions(const std::string &accountId) const;
    Json::Value createCloseQuote(const Json::Value &request) const;
    Json::Value submitClose(const Json::Value &request);
    Json::Value listLimitOrders(const std::string &accountId) const;
    Json::Value createLimitOrderMessage(const Json::Value &request) const;
    Json::Value submitLimitOrder(const Json::Value &request);
    Json::Value createLimitCancelMessage(const Json::Value &request) const;
    Json::Value cancelLimitOrder(const Json::Value &request);
    Json::Value ownerStatus(const std::string &accountId) const;
    Json::Value ownerRegistrationPayload(const Json::Value &request) const;
    Json::Value registerOwner(const Json::Value &request);
    Json::Value agentSetupPayload(const Json::Value &request) const;
    Json::Value createAgentRecord(const Json::Value &request);
    void startLimitWorker();
    void stopLimitWorker();

  private:
    Config config_;
    std::atomic<bool> limitWorkerRunning_{false};
    std::thread limitWorker_;
};

} // namespace afterbell::trading
