#pragma once

#include "oracle/near_publisher.hpp"

#include <cstdint>
#include <memory>
#include <utility>
#include <string>
#include <vector>

namespace afterbell::oracle {

struct PoolPriceSample : PriceAttestation {
    std::uint64_t blockNumber;
    std::uint64_t spotTwapDeviationBps;
    std::uint64_t liquidity;
};

struct SwapVolumeEvent {
    std::string marketId;
    std::string transactionHash;
    std::uint64_t logIndex;
    std::uint64_t blockNumber;
    std::uint64_t timestampMs;
    std::string quoteAmountRaw;
};

class RobinhoodPoolSampler {
  public:
    RobinhoodPoolSampler(std::string rpcUrl, std::string marketRegistryPath,
                         std::string poolRegistryPath, std::uint32_t twapWindowSeconds);
    ~RobinhoodPoolSampler();
    RobinhoodPoolSampler(const RobinhoodPoolSampler &) = delete;
    RobinhoodPoolSampler &operator=(const RobinhoodPoolSampler &) = delete;

    std::vector<PoolPriceSample> sampleAll() const;
    std::pair<std::uint64_t, std::uint64_t> latestBlock() const;
    std::uint64_t blockAtOrBefore(std::uint64_t timestampMs) const;
    std::vector<SwapVolumeEvent> swapVolumeEvents(std::uint64_t fromBlock,
                                                  std::uint64_t toBlock) const;

  private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

} // namespace afterbell::oracle
