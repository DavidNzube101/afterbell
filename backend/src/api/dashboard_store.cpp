#include "api/dashboard_store.hpp"

#include <libpq-fe.h>

#include <algorithm>
#include <stdexcept>
#include <utility>
#include <vector>

namespace afterbell::api {
namespace {

class Connection {
  public:
    explicit Connection(const std::string &url) : connection_(PQconnectdb(url.c_str())) {
        if (!connection_ || PQstatus(connection_) != CONNECTION_OK) {
            const std::string message = connection_ ? PQerrorMessage(connection_) : "Could not allocate PostgreSQL connection";
            if (connection_) PQfinish(connection_);
            connection_ = nullptr;
            throw std::runtime_error(message);
        }
    }
    ~Connection() { if (connection_) PQfinish(connection_); }
    PGresult *query(const std::string &sql, const std::vector<std::string> &parameters = {}) const {
        std::vector<const char *> values;
        values.reserve(parameters.size());
        for (const auto &parameter : parameters) values.push_back(parameter.c_str());
        auto *result = PQexecParams(connection_, sql.c_str(), static_cast<int>(values.size()),
                                    nullptr, values.data(), nullptr, nullptr, 0);
        if (!result) throw std::runtime_error(PQerrorMessage(connection_));
        const auto status = PQresultStatus(result);
        if (status != PGRES_COMMAND_OK && status != PGRES_TUPLES_OK) {
            const std::string message = PQresultErrorMessage(result);
            PQclear(result);
            throw std::runtime_error(message);
        }
        return result;
    }
  private:
    PGconn *connection_{};
};

Json::Value rowsAsJson(PGresult *result) {
    Json::Value output(Json::arrayValue);
    for (int row = 0; row < PQntuples(result); ++row) {
        Json::Value value;
        value["id"] = PQgetvalue(result, row, 0);
        value["event_type"] = PQgetvalue(result, row, 1);
        value["title"] = PQgetvalue(result, row, 2);
        value["message"] = PQgetvalue(result, row, 3);
        value["market_id"] = PQgetvalue(result, row, 4);
        value["side"] = PQgetvalue(result, row, 5);
        value["amount_usdc"] = PQgetvalue(result, row, 6);
        value["transaction_hash"] = PQgetvalue(result, row, 7);
        value["created_at_ms"] = Json::UInt64(std::stoull(PQgetvalue(result, row, 8)));
        output.append(std::move(value));
    }
    PQclear(result);
    return output;
}

} // namespace

DashboardStore::DashboardStore(std::string databaseUrl) : databaseUrl_(std::move(databaseUrl)) {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "CREATE TABLE IF NOT EXISTS near_deposit_events ("
        "network TEXT NOT NULL, receipt_id TEXT NOT NULL, log_index INTEGER NOT NULL, receipt_block_hash TEXT NOT NULL, "
        "transaction_hash TEXT NOT NULL, transaction_block_height BIGINT NOT NULL, contract_id TEXT NOT NULL, token_id TEXT NOT NULL, "
        "account_id TEXT NOT NULL, deposit_id TEXT NOT NULL, sender_id TEXT NOT NULL, amount NUMERIC(78,0) NOT NULL, "
        "event_json JSONB NOT NULL, indexed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), PRIMARY KEY (network,receipt_id,log_index))");
    PQclear(result);
    result = connection.query(
        "CREATE TABLE IF NOT EXISTS afterbell_oracle_mark_history ("
        "network TEXT NOT NULL, market_id TEXT NOT NULL, price_e8 NUMERIC(78,0) NOT NULL, "
        "observed_at_ms BIGINT NOT NULL, sequence BIGINT NOT NULL, "
        "PRIMARY KEY (network, market_id, observed_at_ms))");
    PQclear(result);
    result = connection.query("CREATE INDEX IF NOT EXISTS afterbell_oracle_mark_history_chart_idx "
                              "ON afterbell_oracle_mark_history (network, market_id, observed_at_ms DESC)");
    PQclear(result);
    result = connection.query(
        "CREATE TABLE IF NOT EXISTS afterbell_robinhood_swaps ("
        "network TEXT NOT NULL, market_id TEXT NOT NULL, transaction_hash TEXT NOT NULL, log_index BIGINT NOT NULL, "
        "block_number BIGINT NOT NULL, observed_at_ms BIGINT NOT NULL, quote_amount_raw NUMERIC(78,0) NOT NULL, "
        "PRIMARY KEY (network, market_id, transaction_hash, log_index))");
    PQclear(result);
    result = connection.query("CREATE INDEX IF NOT EXISTS afterbell_robinhood_swaps_chart_idx "
                              "ON afterbell_robinhood_swaps (network, market_id, observed_at_ms)");
    PQclear(result);
    result = connection.query(
        "CREATE TABLE IF NOT EXISTS afterbell_robinhood_swap_cursors ("
        "network TEXT PRIMARY KEY, next_block BIGINT NOT NULL)");
    PQclear(result);
    result = connection.query(
        "CREATE TABLE IF NOT EXISTS afterbell_activity_events ("
        "event_id BIGSERIAL PRIMARY KEY, network TEXT NOT NULL, account_id TEXT NOT NULL, "
        "event_type TEXT NOT NULL, title TEXT NOT NULL, message TEXT NOT NULL, market_id TEXT NOT NULL DEFAULT '', "
        "side TEXT NOT NULL DEFAULT '', amount_usdc NUMERIC(78,0) NOT NULL DEFAULT 0, "
        "transaction_hash TEXT NOT NULL DEFAULT '', created_at TIMESTAMPTZ NOT NULL DEFAULT NOW())");
    PQclear(result);
    result = connection.query("CREATE INDEX IF NOT EXISTS afterbell_activity_events_account_idx "
                              "ON afterbell_activity_events (network, account_id, created_at DESC)");
    PQclear(result);
    result = connection.query(
        "CREATE TABLE IF NOT EXISTS afterbell_notifications ("
        "notification_id BIGSERIAL PRIMARY KEY, event_id BIGINT NOT NULL REFERENCES afterbell_activity_events(event_id) ON DELETE CASCADE, "
        "network TEXT NOT NULL, account_id TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW())");
    PQclear(result);
    result = connection.query("CREATE INDEX IF NOT EXISTS afterbell_notifications_account_idx "
                              "ON afterbell_notifications (network, account_id, created_at DESC)");
    PQclear(result);
}

void DashboardStore::recordOracleMark(const std::string &network, const std::string &market,
                                      std::uint64_t priceE8, std::uint64_t observedAtMs,
                                      std::uint64_t sequence) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "INSERT INTO afterbell_oracle_mark_history (network, market_id, price_e8, observed_at_ms, sequence) "
        "VALUES ($1,$2,$3,$4,$5) ON CONFLICT (network, market_id, observed_at_ms) DO NOTHING",
        {network, market, std::to_string(priceE8), std::to_string(observedAtMs), std::to_string(sequence)});
    PQclear(result);
}

Json::Value DashboardStore::marketHistory(const std::string &network, const std::string &market,
                                         std::uint64_t sinceMs, std::size_t limit) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT price_e8::text, observed_at_ms::text, sequence::text FROM afterbell_oracle_mark_history "
        "WHERE network=$1 AND market_id=$2 AND observed_at_ms >= $3 "
        "ORDER BY observed_at_ms ASC LIMIT $4",
        {network, market, std::to_string(sinceMs), std::to_string(std::min<std::size_t>(limit, 2000))});
    Json::Value output(Json::arrayValue);
    for (int row = 0; row < PQntuples(result); ++row) {
        Json::Value point;
        point["price_e8"] = PQgetvalue(result, row, 0);
        point["timestamp_ms"] = Json::UInt64(std::stoull(PQgetvalue(result, row, 1)));
        point["sequence"] = Json::UInt64(std::stoull(PQgetvalue(result, row, 2)));
        output.append(std::move(point));
    }
    PQclear(result);
    return output;
}

std::optional<std::uint64_t> DashboardStore::marketHistoryStart(const std::string &network,
                                                                const std::string &market) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT MIN(observed_at_ms)::text FROM afterbell_oracle_mark_history WHERE network=$1 AND market_id=$2",
        {network, market});
    if (PQntuples(result) == 0 || PQgetisnull(result, 0, 0)) { PQclear(result); return std::nullopt; }
    const auto value = std::stoull(PQgetvalue(result, 0, 0));
    PQclear(result);
    return value;
}

std::optional<std::uint64_t> DashboardStore::robinhoodSwapCursor(const std::string &network) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query("SELECT next_block::text FROM afterbell_robinhood_swap_cursors WHERE network=$1", {network});
    if (PQntuples(result) == 0) { PQclear(result); return std::nullopt; }
    const auto value = std::stoull(PQgetvalue(result, 0, 0));
    PQclear(result);
    return value;
}

void DashboardStore::setRobinhoodSwapCursor(const std::string &network, std::uint64_t nextBlock) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "INSERT INTO afterbell_robinhood_swap_cursors (network,next_block) VALUES ($1,$2) "
        "ON CONFLICT (network) DO UPDATE SET next_block=EXCLUDED.next_block",
        {network, std::to_string(nextBlock)});
    PQclear(result);
}

void DashboardStore::recordRobinhoodSwap(const std::string &network, const std::string &market,
                                         const std::string &transactionHash, std::uint64_t logIndex,
                                         std::uint64_t blockNumber, std::uint64_t timestampMs,
                                         const std::string &quoteAmountRaw) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "INSERT INTO afterbell_robinhood_swaps "
        "(network,market_id,transaction_hash,log_index,block_number,observed_at_ms,quote_amount_raw) "
        "VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (network,market_id,transaction_hash,log_index) DO NOTHING",
        {network, market, transactionHash, std::to_string(logIndex), std::to_string(blockNumber),
         std::to_string(timestampMs), quoteAmountRaw});
    PQclear(result);
}

Json::Value DashboardStore::marketVolumeHistory(const std::string &network, const std::string &market,
                                               std::uint64_t sinceMs) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT (observed_at_ms / 60000) * 60000 AS bucket_ms, SUM(quote_amount_raw)::text "
        "FROM afterbell_robinhood_swaps WHERE network=$1 AND market_id=$2 AND observed_at_ms >= $3 "
        "GROUP BY bucket_ms ORDER BY bucket_ms ASC",
        {network, market, std::to_string(sinceMs)});
    Json::Value output(Json::arrayValue);
    for (int row = 0; row < PQntuples(result); ++row) {
        Json::Value point;
        point["timestamp_ms"] = Json::UInt64(std::stoull(PQgetvalue(result, row, 0)));
        point["volume_usdg_raw"] = PQgetvalue(result, row, 1);
        output.append(std::move(point));
    }
    PQclear(result);
    return output;
}

std::string DashboardStore::recordActivity(const std::string &network, const std::string &accountId,
                                          const std::string &eventType, const std::string &title,
                                          const std::string &message, const std::string &market,
                                          const std::string &side, const std::string &amountUsdc,
                                          const std::string &transactionHash) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query("BEGIN"); PQclear(result);
    try {
        result = connection.query(
            "INSERT INTO afterbell_activity_events (network,account_id,event_type,title,message,market_id,side,amount_usdc,transaction_hash) "
            "VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING event_id::text",
            {network, accountId, eventType, title, message, market, side, amountUsdc, transactionHash});
        const std::string eventId = PQgetvalue(result, 0, 0); PQclear(result);
        result = connection.query("INSERT INTO afterbell_notifications (event_id,network,account_id) VALUES ($1,$2,$3)",
                                  {eventId, network, accountId});
        PQclear(result); result = connection.query("COMMIT"); PQclear(result);
        return eventId;
    } catch (...) {
        result = connection.query("ROLLBACK"); PQclear(result);
        throw;
    }
}

Json::Value DashboardStore::notifications(const std::string &network, const std::string &accountId) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT n.notification_id::text,e.event_type,e.title,e.message,e.market_id,e.side,e.amount_usdc::text,e.transaction_hash,"
        "(EXTRACT(EPOCH FROM n.created_at)*1000)::bigint::text FROM afterbell_notifications n "
        "JOIN afterbell_activity_events e USING(event_id) WHERE n.network=$1 AND n.account_id=$2 ORDER BY n.created_at DESC LIMIT 200",
        {network, accountId});
    return rowsAsJson(result);
}

void DashboardStore::syncDepositNotifications(const std::string &network, const std::string &accountId) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query("BEGIN"); PQclear(result);
    try {
        // Serialize syncs for the same account so concurrent dashboard requests
        // cannot create duplicate activity/notification rows for one deposit.
        result = connection.query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
                                  {network + ":" + accountId});
        PQclear(result);
        result = connection.query(
            "WITH inserted AS ("
            "INSERT INTO afterbell_activity_events (network,account_id,event_type,title,message,amount_usdc,transaction_hash,created_at) "
            "SELECT d.network,d.account_id,'deposit_credited','USDC deposit credited','A USDC deposit was credited to your Afterbell account.',d.amount,d.transaction_hash,d.indexed_at "
            "FROM near_deposit_events d WHERE d.network=$1 AND d.account_id=$2 AND NOT EXISTS ("
            "SELECT 1 FROM afterbell_activity_events e WHERE e.network=d.network AND e.account_id=d.account_id "
            "AND e.event_type='deposit_credited' AND e.transaction_hash=d.transaction_hash) "
            "RETURNING event_id,network,account_id) "
            "INSERT INTO afterbell_notifications (event_id,network,account_id) SELECT event_id,network,account_id FROM inserted",
            {network, accountId});
        PQclear(result); result = connection.query("COMMIT"); PQclear(result);
    } catch (...) {
        result = connection.query("ROLLBACK"); PQclear(result);
        throw;
    }
}

Json::Value DashboardStore::activityHistory(const std::string &network, const std::string &accountId) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT id,event_type,title,message,market_id,side,amount_usdc,transaction_hash,created_ms FROM ("
        "SELECT event_id::text AS id,event_type,title,message,market_id,side,amount_usdc::text AS amount_usdc,transaction_hash,"
        "(EXTRACT(EPOCH FROM created_at)*1000)::bigint::text AS created_ms FROM afterbell_activity_events WHERE network=$1 AND account_id=$2 "
        "UNION ALL SELECT 'deposit:'||receipt_id||':'||log_index::text,'deposit_credited','USDC deposit credited',sender_id,"
        "'', '',amount::text,transaction_hash,(EXTRACT(EPOCH FROM indexed_at)*1000)::bigint::text FROM near_deposit_events WHERE network=$1 AND account_id=$2"
        ") activity ORDER BY created_ms DESC LIMIT 500", {network, accountId});
    return rowsAsJson(result);
}

bool DashboardStore::deleteNotification(const std::string &network, const std::string &accountId,
                                        const std::string &notificationId) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query("DELETE FROM afterbell_notifications WHERE network=$1 AND account_id=$2 AND notification_id=$3",
                                    {network, accountId, notificationId});
    const bool deleted = std::string(PQcmdTuples(result)) == "1";
    PQclear(result);
    return deleted;
}

void DashboardStore::clearNotifications(const std::string &network, const std::string &accountId) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query("DELETE FROM afterbell_notifications WHERE network=$1 AND account_id=$2", {network, accountId});
    PQclear(result);
}

std::string DashboardStore::marketVolume24h(const std::string &network, const std::string &market) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT COALESCE(SUM(amount_usdc),0)::text FROM afterbell_activity_events "
        "WHERE network=$1 AND market_id=$2 AND event_type IN ('position_opened','position_closed') "
        "AND created_at >= NOW() - INTERVAL '24 hours'", {network, market});
    const std::string value = PQntuples(result) ? PQgetvalue(result, 0, 0) : "0";
    PQclear(result);
    return value;
}

} // namespace afterbell::api
