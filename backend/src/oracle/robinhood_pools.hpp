#pragma once

#include "oracle/near_publisher.hpp"

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace afterbell::oracle {

struct PoolPriceSample : PriceAttestation {
    std::uint64_t blockNumber;
    std::uint64_t spotTwapDeviationBps;
    std::uint64_t liquidity;
};

class RobinhoodPoolSampler {
  public:
    RobinhoodPoolSampler(std::string rpcUrl, std::string marketRegistryPath,
                         std::string poolRegistryPath, std::uint32_t twapWindowSeconds);
    ~RobinhoodPoolSampler();
    RobinhoodPoolSampler(const RobinhoodPoolSampler &) = delete;
    RobinhoodPoolSampler &operator=(const RobinhoodPoolSampler &) = delete;

    std::vector<PoolPriceSample> sampleAll() const;

  private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

} // namespace afterbell::oracle
