#include <drogon/drogon.h>

#include "indexer/worker.hpp"

#include <libpq-fe.h>

#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <memory>
#include <iterator>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>

namespace {

std::optional<std::string> env(const char *name) {
    const auto *value = std::getenv(name);
    if (value == nullptr || *value == '\0') return std::nullopt;
    return std::string(value);
}

std::string requiredEnv(const char *name) {
    const auto value = env(name);
    if (!value) throw std::runtime_error(std::string(name) + " is required");
    return *value;
}

std::optional<std::uint64_t> optionalUintEnv(const char *name) {
    const auto value = env(name);
    if (!value) return std::nullopt;
    std::size_t consumed = 0;
    const auto parsed = std::stoull(*value, &consumed);
    if (consumed != value->size()) {
        throw std::runtime_error(std::string(name) + " must be an unsigned integer");
    }
    return parsed;
}

std::uint64_t uintEnvOr(const char *name, std::uint64_t fallback) {
    return optionalUintEnv(name).value_or(fallback);
}

bool indexerEnabled() {
    const auto value = env("INDEXER_ENABLED").value_or("false");
    if (value == "true" || value == "1" || value == "yes") return true;
    if (value == "false" || value == "0" || value == "no") return false;
    throw std::runtime_error("INDEXER_ENABLED must be true or false");
}

void loadEnvFile(const std::string &path) {
    std::ifstream file(path);
    std::string line;
    while (std::getline(file, line)) {
        const auto first = line.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) continue;
        line = line.substr(first);
        if (line.front() == '#') continue;
        if (line.rfind("export ", 0) == 0) line = line.substr(7);
        const auto equals = line.find('=');
        if (equals == std::string::npos) continue;
        auto name = line.substr(0, equals);
        auto value = line.substr(equals + 1);
        while (!name.empty() && (name.back() == ' ' || name.back() == '\t')) name.pop_back();
        if (name.empty() || std::getenv(name.c_str()) != nullptr) continue;
        if (value.size() >= 2 &&
            ((value.front() == '"' && value.back() == '"') ||
             (value.front() == '\'' && value.back() == '\''))) {
            value = value.substr(1, value.size() - 2);
        }
        setenv(name.c_str(), value.c_str(), 0);
    }
}

unsigned short portFromEnv() {
    const auto value = env("PORT").value_or("8080");
    std::size_t consumed = 0;
    const auto port = std::stoul(value, &consumed);
    if (consumed != value.size() || port == 0 || port > 65535) {
        throw std::runtime_error("PORT must be an integer from 1 to 65535");
    }
    return static_cast<unsigned short>(port);
}

Json::Value loadMarkets(const std::string &path) {
    std::ifstream input(path);
    if (!input) throw std::runtime_error("Could not open market registry file");
    Json::CharReaderBuilder builder;
    Json::Value source;
    std::string errors;
    const std::string contents{std::istreambuf_iterator<char>(input),
                               std::istreambuf_iterator<char>()};
    const auto reader = std::unique_ptr<Json::CharReader>(builder.newCharReader());
    if (!reader->parse(contents.data(), contents.data() + contents.size(), &source, &errors) ||
        !source["Robinhood"].isObject()) {
        throw std::runtime_error("Market registry JSON is invalid");
    }

    Json::Value markets(Json::arrayValue);
    const auto &robinhood = source["Robinhood"];
    for (const auto &symbol : robinhood.getMemberNames()) {
        Json::Value market;
        // A1 uses the ticker as market_id (for example, "NVDA").
        market["id"] = symbol;
        market["symbol"] = symbol;
        market["baseAsset"] = symbol;
        market["quoteAsset"] = "USDC";
        market["sourceChain"] = "robinhood";
        market["sourceChainId"] = 4663;
        market["tokenAddress"] = robinhood[symbol].asString();
        market["status"] = "development";
        market["tradingEnabled"] = false;
        markets.append(std::move(market));
    }
    return markets;
}

bool databaseReady(const std::string &connectionString) {
    return PQping(connectionString.c_str()) == PQPING_OK;
}

Json::Value errorBody(const std::string &code, const std::string &message) {
    Json::Value body;
    body["error"]["code"] = code;
    body["error"]["message"] = message;
    return body;
}

} // namespace

int main() {
    try {
        loadEnvFile(".env");
        loadEnvFile("backend/.env");

        const auto databaseUrl = env("DATABASE_URL");
        if (!databaseUrl) throw std::runtime_error("DATABASE_URL is required");
        const auto network = env("NEAR_NETWORK");
        const auto contractId = env("A1_CONTRACT_ID");
        if (!network || !contractId) {
            throw std::runtime_error("NEAR_NETWORK and A1_CONTRACT_ID are required");
        }
        if (*network != "testnet" && *network != "mainnet") {
            throw std::runtime_error("NEAR_NETWORK must be testnet or mainnet");
        }
        auto registryPath = env("MARKET_REGISTRY_PATH");
        if (!registryPath) {
            std::ifstream fromRepoRoot("backend/rh_stocks_addresses.json");
            registryPath = fromRepoRoot ? "backend/rh_stocks_addresses.json"
                                        : "rh_stocks_addresses.json";
        }
        const auto markets = loadMarkets(*registryPath);

        std::unique_ptr<afterbell::indexer::Worker> indexerWorker;
        if (indexerEnabled()) {
            afterbell::indexer::IndexerConfig indexerConfig{
                *network,
                *contractId,
                requiredEnv("NEAR_USDC_TOKEN_ID"),
                optionalUintEnv("INDEXER_START_BLOCK"),
            };
            indexerConfig.finalityLagBlocks =
                uintEnvOr("INDEXER_FINALITY_LAG_BLOCKS", 1000);
            indexerConfig.rescanOverlapBlocks =
                uintEnvOr("INDEXER_RESCAN_OVERLAP_BLOCKS", 10000);
            indexerWorker = std::make_unique<afterbell::indexer::Worker>(
                std::move(indexerConfig), requiredEnv("INDEXER_NEAR_RPC_URL"),
                requiredEnv("INDEXER_TX_API_URL"), *databaseUrl,
                uintEnvOr("INDEXER_POLL_INTERVAL_MS", 5000));
            indexerWorker->start();
        }

        drogon::app().setThreadNum(4);
        drogon::app().registerHandler(
            "/health/live",
            [](const drogon::HttpRequestPtr &,
               std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                Json::Value body;
                body["status"] = "live";
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            },
            {drogon::Get});
        drogon::app().registerHandler(
            "/health/ready",
            [databaseUrl = *databaseUrl](
                const drogon::HttpRequestPtr &,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!databaseReady(databaseUrl)) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(
                        errorBody("database_unavailable", "Database is unavailable"));
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                Json::Value body;
                body["status"] = "ready";
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            },
            {drogon::Get});
        drogon::app().registerHandler(
            "/v1/markets",
            [markets, network = *network, contractId = *contractId](
                const drogon::HttpRequestPtr &,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                Json::Value body;
                body["network"] = network;
                body["contractId"] = contractId;
                body["markets"] = markets;
                body["tradingEnabled"] = false;
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            },
            {drogon::Get});

        const auto port = portFromEnv();
        std::cerr << "afterbell-api listening on port " << port << " (" << *network
                  << ", indexer " << (indexerWorker ? "enabled" : "disabled") << ")\n";
        drogon::app().addListener("0.0.0.0", port).run();
        if (indexerWorker) indexerWorker->stop();
    } catch (const std::exception &error) {
        std::cerr << "afterbell-api: " << error.what() << "\n";
        return 1;
    }
}
