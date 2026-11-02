#pragma once

#include "indexer/database.hpp"
#include "indexer/fastnear_client.hpp"
#include "indexer/near_rpc.hpp"

#include <cstddef>
#include <cstdint>
#include <string>

namespace afterbell::indexer {

struct IndexerConfig {
    std::string network;
    std::string contractId;
    std::string tokenId;
    std::optional<std::uint64_t> startBlock;
    std::uint64_t finalityLagBlocks{1000};
    std::uint64_t rescanOverlapBlocks{10000};
};

class Indexer {
  public:
    Indexer(IndexerConfig config, NearRpc &rpc, FastNearClient &fastNear,
            Database &database);
    std::size_t indexAvailableRange();
    std::uint64_t cursor() const { return cursor_; }

  private:
    IndexerConfig config_;
    NearRpc &rpc_;
    FastNearClient &fastNear_;
    Database &database_;
    std::uint64_t cursor_{};
    bool initialized_{false};

    std::vector<DepositEvent> eventsInRange(std::uint64_t fromBlock,
                                            std::uint64_t toBlock);
};

} // namespace afterbell::indexer
