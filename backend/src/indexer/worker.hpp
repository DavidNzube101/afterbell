#pragma once

#include "indexer/indexer.hpp"

#include <atomic>
#include <condition_variable>
#include <cstdint>
#include <mutex>
#include <string>
#include <thread>

namespace afterbell::indexer {

class Worker {
  public:
    Worker(IndexerConfig config, std::string rpcUrl, std::string transactionApiUrl,
           std::string databaseUrl, std::uint64_t pollIntervalMs);
    ~Worker();

    Worker(const Worker &) = delete;
    Worker &operator=(const Worker &) = delete;

    void start();
    void stop();

  private:
    void run();
    bool waitOrStop(std::uint64_t durationMs);

    IndexerConfig config_;
    std::string rpcUrl_;
    std::string transactionApiUrl_;
    std::string databaseUrl_;
    std::uint64_t pollIntervalMs_;
    std::atomic<bool> stopping_{false};
    std::mutex waitMutex_;
    std::condition_variable waitCondition_;
    std::thread thread_;
};

} // namespace afterbell::indexer
