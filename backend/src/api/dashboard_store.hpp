#pragma once

#include <json/json.h>

#include <cstdint>
#include <optional>
#include <string>

namespace afterbell::api {

class DashboardStore {
  public:
    explicit DashboardStore(std::string databaseUrl);
    void recordOracleMark(const std::string &network, const std::string &market,
                          std::uint64_t priceE8, std::uint64_t observedAtMs,
                          std::uint64_t sequence) const;
    Json::Value marketHistory(const std::string &network, const std::string &market,
                              std::uint64_t sinceMs, std::size_t limit = 1000) const;
    std::optional<std::uint64_t> marketHistoryStart(const std::string &network,
                                                   const std::string &market) const;
    std::optional<std::uint64_t> robinhoodSwapCursor(const std::string &network) const;
    void setRobinhoodSwapCursor(const std::string &network, std::uint64_t nextBlock) const;
    void recordRobinhoodSwap(const std::string &network, const std::string &market,
                             const std::string &transactionHash, std::uint64_t logIndex,
                             std::uint64_t blockNumber, std::uint64_t timestampMs,
                             const std::string &quoteAmountRaw) const;
    Json::Value marketVolumeHistory(const std::string &network, const std::string &market,
                                    std::uint64_t sinceMs) const;
    std::string recordActivity(const std::string &network, const std::string &accountId,
                               const std::string &eventType, const std::string &title,
                               const std::string &message, const std::string &market,
                               const std::string &side, const std::string &amountUsdc,
                               const std::string &transactionHash) const;
    Json::Value notifications(const std::string &network, const std::string &accountId) const;
    void syncDepositNotifications(const std::string &network, const std::string &accountId) const;
    Json::Value activityHistory(const std::string &network, const std::string &accountId) const;
    bool deleteNotification(const std::string &network, const std::string &accountId,
                            const std::string &notificationId) const;
    void clearNotifications(const std::string &network, const std::string &accountId) const;
    std::string marketVolume24h(const std::string &network, const std::string &market) const;

  private:
    std::string databaseUrl_;
};

} // namespace afterbell::api
