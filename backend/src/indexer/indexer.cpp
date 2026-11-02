#include "indexer/indexer.hpp"

#include "indexer/event.hpp"

#include <algorithm>
#include <limits>
#include <map>
#include <stdexcept>
#include <utility>

namespace afterbell::indexer {
namespace {

std::uint64_t unsignedField(const Json::Value &object, const char *field) {
    const auto &value = object[field];
    if (value.isUInt64()) return value.asUInt64();
    if (value.isInt64() && value.asInt64() >= 0) {
        return static_cast<std::uint64_t>(value.asInt64());
    }
    if (value.isString()) return std::stoull(value.asString());
    throw std::runtime_error(std::string("FastNear omitted numeric field ") + field);
}

std::string stringField(const Json::Value &object, const char *field) {
    const auto &value = object[field];
    if (!value.isString()) {
        throw std::runtime_error(std::string("FastNear omitted string field ") + field);
    }
    return value.asString();
}

Json::Value normalizeTransactionStatus(const Json::Value &transaction) {
    const auto &receipts = transaction["receipts"];
    if (!receipts.isArray()) {
        throw std::runtime_error("FastNear transaction response omitted receipts");
    }
    Json::Value status;
    status["receipts_outcome"] = Json::Value(Json::arrayValue);
    for (const auto &receipt : receipts) {
        const auto &execution = receipt["execution_outcome"];
        if (!execution.isObject() || !execution["outcome"].isObject()) continue;
        Json::Value normalized;
        normalized["id"] = execution["id"];
        normalized["block_hash"] = execution["block_hash"];
        normalized["outcome"] = execution["outcome"];
        status["receipts_outcome"].append(std::move(normalized));
    }
    return status;
}

} // namespace

Indexer::Indexer(IndexerConfig config, NearRpc &rpc, FastNearClient &fastNear,
                 Database &database)
    : config_(std::move(config)), rpc_(rpc), fastNear_(fastNear), database_(database) {
    if (config_.network.empty() || config_.contractId.empty() || config_.tokenId.empty()) {
        throw std::invalid_argument("NEAR network, A1 contract, and USDC token are required");
    }
}

std::vector<DepositEvent> Indexer::eventsInRange(std::uint64_t fromBlock,
                                                 std::uint64_t toBlock) {
    const auto accountTransactions = fastNear_.accountTransactions(
        config_.contractId, fromBlock, toBlock);
    std::map<std::string, std::uint64_t> transactionHeights;
    std::vector<std::string> hashes;
    hashes.reserve(accountTransactions.size());
    for (const auto &transaction : accountTransactions) {
        if (transactionHeights.emplace(transaction.hash, transaction.blockHeight).second) {
            hashes.push_back(transaction.hash);
        }
    }

    const auto transactions = fastNear_.transactions(hashes);
    if (transactions.size() != hashes.size()) {
        throw std::runtime_error("FastNear transaction history was incomplete");
    }

    std::vector<DepositEvent> events;
    for (const auto &transaction : transactions) {
        const auto &rawTransaction = transaction["transaction"];
        const auto txHash = stringField(rawTransaction, "hash");
        const auto height = transactionHeights.find(txHash);
        if (height == transactionHeights.end()) {
            throw std::runtime_error("FastNear returned a transaction outside the requested account range");
        }
        if (!isUsdcTransferCall(rawTransaction, config_.tokenId)) continue;

        const auto normalizedStatus = normalizeTransactionStatus(transaction);
        auto receiptEvents = parseDepositEvents(
            normalizedStatus, config_.network, config_.contractId, config_.tokenId,
            txHash, height->second);
        events.insert(events.end(), std::make_move_iterator(receiptEvents.begin()),
                      std::make_move_iterator(receiptEvents.end()));
    }
    return events;
}

std::size_t Indexer::indexAvailableRange() {
    if (!initialized_) {
        cursor_ = database_.ensureCursor(config_.network, config_.startBlock);
        initialized_ = true;
    }

    const auto head = unsignedField(rpc_.finalizedHead()["header"], "height");
    if (head <= config_.finalityLagBlocks) return 0;
    const auto targetHeight = head - config_.finalityLagBlocks;
    if (targetHeight <= cursor_) return 0;

    const auto earliest = config_.startBlock.value_or(0);
    const auto overlapStart = cursor_ > config_.rescanOverlapBlocks
                                  ? cursor_ - config_.rescanOverlapBlocks + 1
                                  : earliest;
    const auto fromBlock = std::max(earliest, overlapStart);
    const auto events = eventsInRange(fromBlock, targetHeight);
    const auto previousCursor = cursor_;
    database_.saveFinalizedBlock(config_.network, targetHeight, previousCursor, events);
    cursor_ = targetHeight;
    return events.size();
}

} // namespace afterbell::indexer
