#include "indexer/database.hpp"

#include <json/json.h>

#include <limits>
#include <stdexcept>
#include <utility>

namespace afterbell::indexer {
namespace {

std::string compactJson(const Json::Value &value) {
    Json::StreamWriterBuilder builder;
    builder["indentation"] = "";
    return Json::writeString(builder, value);
}

std::string resultError(PGconn *connection, PGresult *result = nullptr) {
    const char *message = result == nullptr ? PQerrorMessage(connection)
                                             : PQresultErrorMessage(result);
    return message == nullptr ? "PostgreSQL operation failed" : std::string(message);
}

std::string u64(std::uint64_t value) { return std::to_string(value); }

} // namespace

Database::Database(const std::string &connectionString) {
    connection_ = PQconnectdb(connectionString.c_str());
    if (connection_ == nullptr || PQstatus(connection_) != CONNECTION_OK) {
        const auto message = connection_ == nullptr
                                 ? "Could not allocate PostgreSQL connection"
                                 : resultError(connection_);
        if (connection_ != nullptr) PQfinish(connection_);
        connection_ = nullptr;
        throw std::runtime_error(message);
    }
    execute("CREATE TABLE IF NOT EXISTS near_indexer_cursors ("
            "network TEXT PRIMARY KEY, block_height BIGINT NOT NULL CHECK (block_height >= -1), "
            "updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW())");
    execute("CREATE TABLE IF NOT EXISTS near_deposit_events ("
            "network TEXT NOT NULL, receipt_id TEXT NOT NULL, log_index INTEGER NOT NULL, "
            "receipt_block_hash TEXT NOT NULL, transaction_hash TEXT NOT NULL, "
            "transaction_block_height BIGINT NOT NULL, contract_id TEXT NOT NULL, "
            "token_id TEXT NOT NULL, account_id TEXT NOT NULL, deposit_id TEXT NOT NULL, "
            "sender_id TEXT NOT NULL, amount NUMERIC(78,0) NOT NULL, event_json JSONB NOT NULL, "
            "indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), "
            "PRIMARY KEY (network, receipt_id, log_index))");
    execute("CREATE INDEX IF NOT EXISTS near_deposit_events_account_idx "
            "ON near_deposit_events (network, account_id, transaction_block_height)");
}

Database::~Database() {
    if (connection_ != nullptr) PQfinish(connection_);
}

void Database::execute(const std::string &sql) {
    PGresult *result = PQexec(connection_, sql.c_str());
    if (result == nullptr) throw std::runtime_error(resultError(connection_));
    const auto status = PQresultStatus(result);
    if (status != PGRES_COMMAND_OK && status != PGRES_TUPLES_OK) {
        const auto message = resultError(connection_, result);
        PQclear(result);
        throw std::runtime_error(message);
    }
    PQclear(result);
}

PGresult *Database::query(const std::string &sql,
                          const std::vector<std::string> &parameters) {
    std::vector<const char *> values;
    values.reserve(parameters.size());
    for (const auto &parameter : parameters) values.push_back(parameter.c_str());
    PGresult *result = PQexecParams(connection_, sql.c_str(),
                                    static_cast<int>(values.size()), nullptr,
                                    values.data(), nullptr, nullptr, 0);
    if (result == nullptr) throw std::runtime_error(resultError(connection_));
    const auto status = PQresultStatus(result);
    if (status != PGRES_COMMAND_OK && status != PGRES_TUPLES_OK) {
        const auto message = resultError(connection_, result);
        PQclear(result);
        throw std::runtime_error(message);
    }
    return result;
}

std::uint64_t Database::ensureCursor(const std::string &network,
                                     std::optional<std::uint64_t> startBlock) {
    auto *result = query("SELECT block_height FROM near_indexer_cursors WHERE network = $1",
                         {network});
    if (PQntuples(result) > 0) {
        const auto height = std::stoll(PQgetvalue(result, 0, 0));
        PQclear(result);
        return height < 0 ? 0 : static_cast<std::uint64_t>(height);
    }
    PQclear(result);
    if (!startBlock) {
        throw std::runtime_error(
            "INDEXER_START_BLOCK is required when the indexer database has no cursor");
    }
    if (*startBlock == 0) {
        throw std::invalid_argument("INDEXER_START_BLOCK must be greater than zero");
    }
    if (*startBlock > static_cast<std::uint64_t>(std::numeric_limits<std::int64_t>::max())) {
        throw std::invalid_argument("INDEXER_START_BLOCK is out of range");
    }
    const auto initialHeight = static_cast<std::int64_t>(*startBlock - 1);
    result = query(
        "INSERT INTO near_indexer_cursors (network, block_height) VALUES ($1, $2) "
        "ON CONFLICT (network) DO NOTHING",
        {network, std::to_string(initialHeight)});
    PQclear(result);

    result = query("SELECT block_height FROM near_indexer_cursors WHERE network = $1",
                   {network});
    if (PQntuples(result) != 1) {
        PQclear(result);
        throw std::runtime_error("Could not initialize indexer block cursor");
    }
    const auto height = std::stoll(PQgetvalue(result, 0, 0));
    PQclear(result);
    return height < 0 ? 0 : static_cast<std::uint64_t>(height);
}

void Database::saveFinalizedBlock(const std::string &network,
                                  std::uint64_t height,
                                  std::uint64_t expectedCursor,
                                  const std::vector<DepositEvent> &events) {
    execute("BEGIN");
    try {
        for (const auto &event : events) {
            const auto eventJson = compactJson(event.eventJson);
            auto *result = query(
                "INSERT INTO near_deposit_events (network, receipt_id, log_index, "
                "receipt_block_hash, transaction_hash, transaction_block_height, "
                "contract_id, token_id, account_id, deposit_id, sender_id, amount, event_json) "
                "VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12::numeric, $13::jsonb) "
                "ON CONFLICT (network, receipt_id, log_index) DO NOTHING",
                {event.network, event.receiptId, std::to_string(event.logIndex),
                 event.receiptBlockHash, event.transactionHash,
                 u64(event.transactionBlockHeight), event.contractId, event.tokenId,
                 event.accountId, event.depositId, event.senderId, event.amount, eventJson});
            PQclear(result);
        }

        auto *result = query(
            "UPDATE near_indexer_cursors SET block_height = $2, updated_at = NOW() "
            "WHERE network = $1 AND block_height = $3",
            {network, u64(height), u64(expectedCursor)});
        const auto updatedRows = PQcmdTuples(result);
        PQclear(result);
        if (updatedRows[0] == '0') {
            result = query("SELECT block_height FROM near_indexer_cursors WHERE network = $1",
                           {network});
            const bool alreadyProcessed = PQntuples(result) == 1 &&
                std::stoull(PQgetvalue(result, 0, 0)) >= height;
            PQclear(result);
            if (!alreadyProcessed) {
                throw std::runtime_error("Indexer cursor changed while committing a block");
            }
        }
        execute("COMMIT");
    } catch (...) {
        try {
            execute("ROLLBACK");
        } catch (...) {
        }
        throw;
    }
}

} // namespace afterbell::indexer
