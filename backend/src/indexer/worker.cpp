#include "indexer/worker.hpp"

#include "indexer/database.hpp"
#include "indexer/fastnear_client.hpp"
#include "indexer/near_rpc.hpp"

#include <chrono>
#include <iostream>
#include <stdexcept>
#include <utility>

namespace afterbell::indexer {

Worker::Worker(IndexerConfig config, std::string rpcUrl,
               std::string transactionApiUrl, std::string databaseUrl,
               std::uint64_t pollIntervalMs)
    : config_(std::move(config)), rpcUrl_(std::move(rpcUrl)),
      transactionApiUrl_(std::move(transactionApiUrl)),
      databaseUrl_(std::move(databaseUrl)), pollIntervalMs_(pollIntervalMs) {
    if (pollIntervalMs_ < 500 || config_.finalityLagBlocks == 0 ||
        config_.rescanOverlapBlocks == 0) {
        throw std::invalid_argument("Indexer polling and overlap settings must be positive");
    }
}

Worker::~Worker() { stop(); }

void Worker::start() {
    if (thread_.joinable()) throw std::logic_error("Indexer worker already started");
    stopping_.store(false);
    thread_ = std::thread([this] { run(); });
}

void Worker::stop() {
    stopping_.store(true);
    waitCondition_.notify_all();
    if (thread_.joinable()) thread_.join();
}

bool Worker::waitOrStop(std::uint64_t durationMs) {
    std::unique_lock<std::mutex> lock(waitMutex_);
    return waitCondition_.wait_for(
        lock, std::chrono::milliseconds(durationMs),
        [this] { return stopping_.load(); });
}

void Worker::run() {
    while (!stopping_.load()) {
        try {
            NearRpc rpc(rpcUrl_);
            const auto chainId = rpc.chainId();
            if (chainId != config_.network) {
                throw std::runtime_error("Indexer RPC is connected to " + chainId +
                                         " but configured network is " + config_.network);
            }
            FastNearClient fastNear(transactionApiUrl_);
            Database database(databaseUrl_);
            Indexer indexer(config_, rpc, fastNear, database);
            std::cerr << "afterbell-indexer background worker started for "
                      << config_.network << "\n";

            while (!stopping_.load()) {
                try {
                    const auto previousCursor = indexer.cursor();
                    const auto indexedEvents = indexer.indexAvailableRange();
                    if (indexer.cursor() > previousCursor) {
                        std::cerr << "scanned A1 history through block "
                                  << indexer.cursor()
                                  << "; deposit events indexed: " << indexedEvents << "\n";
                    }
                } catch (const std::exception &error) {
                    std::cerr << "indexer pass failed; cursor retained for retry: "
                              << error.what() << "\n";
                    break;
                }
                if (waitOrStop(pollIntervalMs_)) return;
            }
        } catch (const std::exception &error) {
            std::cerr << "indexer worker unavailable; retrying: " << error.what() << "\n";
        }
        if (stopping_.load() || waitOrStop(5000)) return;
    }
}

} // namespace afterbell::indexer
