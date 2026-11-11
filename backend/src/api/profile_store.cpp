#include "api/profile_store.hpp"

#include <libpq-fe.h>

#include <cctype>
#include <regex>
#include <stdexcept>
#include <utility>
#include <vector>

namespace afterbell::api {
namespace {

class Connection {
  public:
    explicit Connection(const std::string &url) : connection_(PQconnectdb(url.c_str())) {
        if (!connection_ || PQstatus(connection_) != CONNECTION_OK) {
            const std::string message = connection_ ? PQerrorMessage(connection_)
                                                    : "Could not allocate PostgreSQL connection";
            if (connection_) PQfinish(connection_);
            connection_ = nullptr;
            throw std::runtime_error(message);
        }
    }
    ~Connection() { if (connection_) PQfinish(connection_); }
    Connection(const Connection &) = delete;
    Connection &operator=(const Connection &) = delete;

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

void validateAccount(const std::string &accountId) {
    if (accountId.empty() || accountId.size() > 128)
        throw std::invalid_argument("account_id must contain 1 to 128 characters");
}

void validateUsername(const std::string &username) {
    static const std::regex pattern("^[a-z0-9_]{3,20}$");
    if (!std::regex_match(username, pattern))
        throw std::invalid_argument("Use 3–20 lowercase letters, numbers, or underscores for your username.");
}

std::string normalizeEmail(std::string email) {
    const auto first = email.find_first_not_of(" \t\r\n");
    const auto last = email.find_last_not_of(" \t\r\n");
    if (first == std::string::npos) throw std::invalid_argument("Enter the email used for your waitlist claim.");
    email = email.substr(first, last - first + 1);
    for (auto &character : email) character = static_cast<char>(std::tolower(static_cast<unsigned char>(character)));
    static const std::regex pattern("^[^\\s@]+@[^\\s@]+\\.[^\\s@]{2,}$");
    if (email.size() > 254 || !std::regex_match(email, pattern))
        throw std::invalid_argument("Enter a valid email address.");
    return email;
}

} // namespace

ProfileStore::ProfileStore(std::string databaseUrl) : databaseUrl_(std::move(databaseUrl)) {
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "CREATE TABLE IF NOT EXISTS afterbell_user_profiles ("
        "account_id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, "
        "created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW())");
    PQclear(result);
}

Json::Value ProfileStore::checkUsername(const std::string &accountId, const std::string &username) const {
    Json::Value response;
    try {
        validateAccount(accountId);
        validateUsername(username);
    } catch (const std::invalid_argument &) {
        response["valid"] = false;
        response["available"] = false;
        return response;
    }

    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT NOT EXISTS ("
        "SELECT 1 FROM afterbell_user_profiles p WHERE lower(p.username) = lower($1) AND p.account_id <> $2 "
        "UNION ALL "
        "SELECT 1 FROM waitlist_signups w WHERE lower(w.username) = lower($1) "
        "AND NOT EXISTS (SELECT 1 FROM afterbell_user_profiles p WHERE p.account_id = $2 AND lower(p.username) = lower($1))"
        ") AND NOT EXISTS (SELECT 1 FROM afterbell_user_profiles p WHERE p.account_id = $2 AND lower(p.username) <> lower($1))",
        {username, accountId});
    response["valid"] = true;
    response["available"] = PQntuples(result) == 1 && std::string(PQgetvalue(result, 0, 0)) == "t";
    PQclear(result);
    return response;
}

Json::Value ProfileStore::claimUsername(const std::string &accountId, const std::string &username) const {
    validateAccount(accountId);
    validateUsername(username);
    return persist(accountId, username, false);
}

Json::Value ProfileStore::reclaimUsername(const std::string &accountId, const std::string &email) const {
    validateAccount(accountId);
    const auto normalizedEmail = normalizeEmail(email);
    Connection connection(databaseUrl_);
    auto *result = connection.query(
        "SELECT username FROM waitlist_signups WHERE lower(email) = $1 "
        "ORDER BY created_at DESC LIMIT 1", {normalizedEmail});
    if (PQntuples(result) == 0) {
        PQclear(result);
        throw std::invalid_argument("No reserved username was found for that email.");
    }
    const std::string username = PQgetvalue(result, 0, 0);
    PQclear(result);
    validateUsername(username);
    return persist(accountId, username, true);
}

Json::Value ProfileStore::persist(const std::string &accountId, const std::string &username,
                                  bool fromWaitlist) const {
    Connection connection(databaseUrl_);
    auto *result = connection.query("BEGIN");
    PQclear(result);
    try {
        result = connection.query("SELECT username FROM afterbell_user_profiles WHERE account_id = $1", {accountId});
        if (PQntuples(result) != 0) {
            const bool alreadyClaimed = std::string(PQgetvalue(result, 0, 0)) == username;
            PQclear(result);
            if (alreadyClaimed) {
                result = connection.query("COMMIT");
                PQclear(result);
                Json::Value response;
                response["username"] = username;
                response["status"] = "already_claimed";
                response["reclaimed"] = fromWaitlist;
                return response;
            }
            throw UsernameUnavailable("This wallet already has a username.");
        }
        PQclear(result);
        if (!fromWaitlist) {
            result = connection.query(
                "SELECT 1 FROM waitlist_signups WHERE lower(username) = lower($1) LIMIT 1", {username});
            const bool reserved = PQntuples(result) != 0;
            PQclear(result);
            if (reserved) throw UsernameUnavailable("That username is already reserved.");
        }
        result = connection.query(
            "INSERT INTO afterbell_user_profiles (account_id, username) VALUES ($1, $2) "
            "ON CONFLICT DO NOTHING RETURNING username", {accountId, username});
        if (PQntuples(result) != 0) {
            const std::string savedUsername = PQgetvalue(result, 0, 0);
            PQclear(result);
            result = connection.query("COMMIT");
            PQclear(result);
            Json::Value response;
            response["username"] = savedUsername;
            response["status"] = "claimed";
            response["reclaimed"] = fromWaitlist;
            return response;
        }
        PQclear(result);
        result = connection.query("SELECT username FROM afterbell_user_profiles WHERE account_id = $1", {accountId});
        if (PQntuples(result) != 0 && std::string(PQgetvalue(result, 0, 0)) == username) {
            PQclear(result);
            result = connection.query("COMMIT");
            PQclear(result);
            Json::Value response;
            response["username"] = username;
            response["status"] = "already_claimed";
            response["reclaimed"] = fromWaitlist;
            return response;
        }
        PQclear(result);
        throw UsernameUnavailable("That username is unavailable or this wallet already has a username.");
    } catch (...) {
        result = connection.query("ROLLBACK");
        PQclear(result);
        throw;
    }
}

} // namespace afterbell::api
