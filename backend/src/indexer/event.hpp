#pragma once

#include <json/json.h>

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace afterbell::indexer {

struct DepositEvent {
    std::string network;
    std::string receiptId;
    std::int32_t logIndex{};
    std::string receiptBlockHash;
    std::string transactionHash;
    std::uint64_t transactionBlockHeight{};
    std::string contractId;
    std::string tokenId;
    std::string accountId;
    std::string depositId;
    std::string senderId;
    std::string amount;
    Json::Value eventJson;
};

std::optional<Json::Value> parseEventJsonLog(const std::string &line);

std::vector<DepositEvent> parseDepositEvents(
    const Json::Value &transactionStatus,
    const std::string &network,
    const std::string &contractId,
    const std::string &tokenId,
    const std::string &transactionHash,
    std::uint64_t transactionBlockHeight);

bool isUsdcTransferCall(const Json::Value &transaction,
                        const std::string &tokenId);

} // namespace afterbell::indexer
