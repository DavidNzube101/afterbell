#include "indexer/event.hpp"

#include <cstdlib>
#include <iostream>
#include <stdexcept>
#include <string>

namespace {

void require(bool condition, const std::string &message) {
    if (!condition) throw std::runtime_error(message);
}

Json::Value eventLog(const std::string &receiptId, const std::string &amount) {
    Json::Value receipt;
    receipt["id"] = receiptId;
    receipt["block_hash"] = "final-block-hash";
    Json::Value outcome;
    outcome["executor_id"] = "a1.testnet";
    outcome["status"]["SuccessValue"] = "";
    outcome["logs"] = Json::Value(Json::arrayValue);
    Json::Value event;
    event["standard"] = "afterbell";
    event["version"] = "1.0.0";
    event["event"] = "deposit_credited";
    Json::Value data(Json::arrayValue);
    Json::Value entry;
    entry["account_id"] = "solana:wallet-record";
    entry["deposit_id"] = "same-quote-id";
    entry["sender_id"] = "solver.near";
    entry["amount"] = amount;
    data.append(entry);
    event["data"] = data;
    outcome["logs"].append("EVENT_JSON:" + Json::writeString(Json::StreamWriterBuilder(), event));
    receipt["outcome"] = outcome;
    return receipt;
}

void testExtractsSuccessfulReceiptEventsWithStableIdentity() {
    Json::Value status;
    status["receipts_outcome"] = Json::Value(Json::arrayValue);
    status["receipts_outcome"].append(eventLog("receipt-a", "1000000"));
    status["receipts_outcome"].append(eventLog("receipt-b", "2500000"));

    const auto events = afterbell::indexer::parseDepositEvents(
        status, "testnet", "a1.testnet", "usdc.testnet", "transaction-hash", 42);

    require(events.size() == 2, "expected one parsed event per successful receipt");
    require(events[0].receiptId == "receipt-a" && events[1].receiptId == "receipt-b",
            "receipt IDs must distinguish transfers that reuse a deposit ID");
    require(events[0].depositId == events[1].depositId,
            "deposit IDs may legitimately repeat across receipts");
    require(events[0].logIndex == 0 && events[1].logIndex == 0,
            "log index must preserve the event position within each receipt");
    require(events[0].amount == "1000000" && events[1].amount == "2500000",
            "raw token amounts must remain exact decimal strings");
    require(events[0].transactionBlockHeight == 42,
            "transaction inclusion height should be retained");
}

void testIgnoresFailedOrForeignContractOutcomes() {
    Json::Value status;
    status["receipts_outcome"] = Json::Value(Json::arrayValue);
    auto failed = eventLog("failed-receipt", "1000000");
    failed["outcome"]["status"] = Json::Value(Json::objectValue);
    failed["outcome"]["status"]["Failure"] = "contract panicked";
    status["receipts_outcome"].append(failed);
    auto foreign = eventLog("foreign-receipt", "1000000");
    foreign["outcome"]["executor_id"] = "other.testnet";
    status["receipts_outcome"].append(foreign);

    const auto events = afterbell::indexer::parseDepositEvents(
        status, "testnet", "a1.testnet", "usdc.testnet", "tx", 1);
    require(events.empty(), "failed and foreign receipt logs must not be indexed");
}

void testFiltersOnlyUsdcTransferCalls() {
    Json::Value transaction;
    transaction["receiver_id"] = "usdc.testnet";
    transaction["actions"] = Json::Value(Json::arrayValue);
    Json::Value action;
    action["FunctionCall"]["method_name"] = "ft_transfer_call";
    transaction["actions"].append(action);
    require(afterbell::indexer::isUsdcTransferCall(transaction, "usdc.testnet"),
            "USDC transfer_call should be picked up");
    require(!afterbell::indexer::isUsdcTransferCall(transaction, "other.testnet"),
            "other fungible token contracts should be skipped");
    transaction["actions"][0]["FunctionCall"]["method_name"] = "ft_transfer";
    require(!afterbell::indexer::isUsdcTransferCall(transaction, "usdc.testnet"),
            "plain ft_transfer does not invoke A1's receiver callback");
}

} // namespace

int main() {
    try {
        testExtractsSuccessfulReceiptEventsWithStableIdentity();
        testIgnoresFailedOrForeignContractOutcomes();
        testFiltersOnlyUsdcTransferCalls();
    } catch (const std::exception &error) {
        std::cerr << error.what() << '\n';
        return EXIT_FAILURE;
    }
    std::cout << "3 indexer tests passed\n";
    return EXIT_SUCCESS;
}
