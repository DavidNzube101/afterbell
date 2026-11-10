#pragma once

#include <json/json.h>

#include <atomic>
#include <condition_variable>
#include <cstdint>
#include <memory>
#include <mutex>
#include <string>
#include <thread>

namespace afterbell::oracle {

class RobinhoodPoolSampler;
class NearPublisher;

struct WorkerConfig {
    std::string robinhoodRpcUrl;
    std::string nearRpcUrl;
    std::string contractId;
    std::string network;
    std::string relayerAccountId;
    std::string relayerPrivateKey;
    std::string evmSignerPrivateKey;
    std::string marketRegistryPath;
    std::string poolRegistryPath;
    std::uint32_t pollIntervalMs{15'000};
    std::uint32_t twapWindowSeconds{300};
};

class Worker {
  public:
    explicit Worker(WorkerConfig config);
    ~Worker();
    Worker(const Worker &) = delete;
    Worker &operator=(const Worker &) = delete;

    void start();
    void stop();
    Json::Value status() const;

  private:
    void run();
    WorkerConfig config_;
    std::unique_ptr<RobinhoodPoolSampler> sampler_;
    std::unique_ptr<NearPublisher> publisher_;
    std::atomic<bool> stopping_{false};
    std::thread thread_;
    mutable std::mutex statusMutex_;
    std::condition_variable wake_;
    Json::Value status_;
};

} // namespace afterbell::oracle
