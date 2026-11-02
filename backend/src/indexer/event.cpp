#include "indexer/event.hpp"

#include <algorithm>
#include <cctype>
#include <limits>
#include <memory>

namespace afterbell::indexer {
namespace {

constexpr char kEventPrefix[] = "EVENT_JSON:";
constexpr char kDepositEvent[] = "deposit_credited";

bool isSuccess(const Json::Value &status) {
    if (!status.isObject()) return false;
    return status.isMember("SuccessValue") || status.isMember("SuccessReceiptId") ||
           status.isMember("SuccessDelegateAction");
}

std::string stringField(const Json::Value &value, const char *field) {
    const auto &fieldValue = value[field];
    return fieldValue.isString() ? fieldValue.asString() : std::string{};
}

bool isUnsignedDecimal(const std::string &value) {
    return !value.empty() &&
           std::all_of(value.begin(), value.end(), [](unsigned char character) {
               return std::isdigit(character) != 0;
           });
}

} // namespace

std::optional<Json::Value> parseEventJsonLog(const std::string &line) {
    if (line.compare(0, sizeof(kEventPrefix) - 1, kEventPrefix) != 0) {
        return std::nullopt;
    }

    Json::CharReaderBuilder builder;
    builder["collectComments"] = false;
    Json::Value event;
    std::string errors;
    const auto *begin = line.data() + sizeof(kEventPrefix) - 1;
    const auto *end = line.data() + line.size();
    const auto reader = std::unique_ptr<Json::CharReader>(builder.newCharReader());
    if (!reader->parse(begin, end, &event, &errors) || !event.isObject()) {
        return std::nullopt;
    }
    return event;
}

std::vector<DepositEvent> parseDepositEvents(
    const Json::Value &transactionStatus,
    const std::string &network,
    const std::string &contractId,
    const std::string &tokenId,
    const std::string &transactionHash,
    std::uint64_t transactionBlockHeight) {
    std::vector<DepositEvent> events;
    const auto &outcomes = transactionStatus["receipts_outcome"];
    if (!outcomes.isArray()) return events;

    for (const auto &receipt : outcomes) {
        const auto &outcome = receipt["outcome"];
        if (stringField(outcome, "executor_id") != contractId ||
            !isSuccess(outcome["status"])) {
            continue;
        }
        const auto &logs = outcome["logs"];
        if (!logs.isArray()) continue;

        for (Json::ArrayIndex index = 0; index < logs.size(); ++index) {
            if (index > static_cast<Json::ArrayIndex>(std::numeric_limits<std::int32_t>::max())) {
                break;
            }
            const auto event = parseEventJsonLog(logs[index].asString());
            if (!event || stringField(*event, "standard") != "afterbell" ||
                stringField(*event, "version") != "1.0.0" ||
                stringField(*event, "event") != kDepositEvent) {
                continue;
            }

            const auto &data = (*event)["data"];
            if (!data.isArray()) continue;
            for (const auto &entry : data) {
                const auto accountId = stringField(entry, "account_id");
                const auto depositId = stringField(entry, "deposit_id");
                const auto senderId = stringField(entry, "sender_id");
                const auto amount = stringField(entry, "amount");
                if (accountId.empty() || depositId.empty() || senderId.empty() ||
                    !isUnsignedDecimal(amount)) {
                    continue;
                }

                events.push_back({
                    network,
                    stringField(receipt, "id"),
                    static_cast<std::int32_t>(index),
                    stringField(receipt, "block_hash"),
                    transactionHash,
                    transactionBlockHeight,
                    contractId,
                    tokenId,
                    accountId,
                    depositId,
                    senderId,
                    amount,
                    *event,
                });
            }
        }
    }
    return events;
}

bool isUsdcTransferCall(const Json::Value &transaction,
                        const std::string &tokenId) {
    if (stringField(transaction, "receiver_id") != tokenId) return false;
    const auto &actions = transaction["actions"];
    if (!actions.isArray()) return false;
    for (const auto &action : actions) {
        if (stringField(action["FunctionCall"], "method_name") == "ft_transfer_call") {
            return true;
        }
    }
    return false;
}

} // namespace afterbell::indexer
