#pragma once

#include <cstdint>
#include <mutex>
#include <string>

namespace afterbell::oracle {

struct PriceAttestation {
    std::string marketId;
    std::uint64_t price;
    std::uint64_t spotPrice;
    std::uint16_t confidenceBps;
    std::uint64_t observedAtMs;
    std::string sourceRound;
};

class NearPublisher {
  public:
    NearPublisher(std::string rpcUrl, std::string contractId,
                  std::string accountId, std::string privateKey,
                  std::string oracleSignerAddress,
                  std::string oracleSignerPrivateKey,
                  std::string expectedNetwork);
    ~NearPublisher();
    NearPublisher(const NearPublisher &) = delete;
    NearPublisher &operator=(const NearPublisher &) = delete;

    std::string publish(const PriceAttestation &price);
    const std::string &accountId() const noexcept { return accountId_; }
    bool verifyContractOracleSigner() const;

  private:
    std::string rpcUrl_;
    std::string contractId_;
    std::string accountId_;
    std::string privateKey_;
    std::string oracleSignerAddress_;
    std::string oracleSignerPrivateKey_;
    std::string expectedNetwork_;
    mutable std::mutex mutex_;
};

} // namespace afterbell::oracle
