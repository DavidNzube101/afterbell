#pragma once

#include <cstdint>
#include <mutex>
#include <optional>
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

struct MarketMark {
    std::uint64_t priceE8;
    std::uint64_t observedAtMs;
    std::uint64_t sequence;
};

class NearPublisher {
  public:
    NearPublisher(std::string rpcUrl, std::string transactionRpcUrl,
                  std::string contractId,
                  std::string accountId, std::string privateKey,
                  std::string oracleSignerAddress,
                  std::string oracleSignerPrivateKey,
                  std::string expectedNetwork);
    ~NearPublisher();
    NearPublisher(const NearPublisher &) = delete;
    NearPublisher &operator=(const NearPublisher &) = delete;

    std::string publish(const PriceAttestation &price);
    std::optional<MarketMark> marketMark(const std::string &marketId) const;
    const std::string &accountId() const noexcept { return accountId_; }
    bool verifyContractOracleSigner() const;

  private:
    std::string rpcUrl_;
    std::string transactionRpcUrl_;
    std::string contractId_;
    std::string accountId_;
    std::string privateKey_;
    std::string oracleSignerAddress_;
    std::string oracleSignerPrivateKey_;
    std::string expectedNetwork_;
    mutable std::mutex mutex_;
};

} // namespace afterbell::oracle
