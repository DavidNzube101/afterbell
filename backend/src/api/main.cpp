#include <drogon/drogon.h>

#include "http/request_logger.hpp"
#include "ai/otto.hpp"
#include "api/profile_store.hpp"
#include "api/dashboard_store.hpp"
#include "indexer/worker.hpp"
#include "oracle/worker.hpp"
#include "trading/service.hpp"

#include <libpq-fe.h>

#include <cstdint>
#include <chrono>
#include <cstdlib>
#include <fstream>
#include <future>
#include <iostream>
#include <iterator>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

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

std::string nearRpcUrlForNetwork(const std::string &network) {
    if (network != "testnet" && network != "mainnet")
        throw std::runtime_error("NEAR_NETWORK must be testnet or mainnet");
    const auto provider = env("NEAR_RPC_PROVIDER").value_or("");
    const bool useFastNear = provider == "fastnear" ||
                             (provider.empty() && env("FASTNEAR_API_KEY").has_value());
    if (provider != "" && provider != "fastnear" && provider != "configured")
        throw std::runtime_error("NEAR_RPC_PROVIDER must be fastnear or configured");
    if (useFastNear) {
        std::cerr << "NEAR read RPC provider: FastNear (" << network << ")\n";
        return network == "testnet" ? "https://rpc.testnet.fastnear.com"
                                     : "https://rpc.mainnet.fastnear.com";
    }
    std::cerr << "NEAR read RPC provider: configured endpoint (" << network << ")\n";
    return requiredEnv(network == "testnet" ? "NEAR_TESTNET_RPC_URL" : "NEAR_RPC_URL");
}

std::string nearTransactionRpcUrlForNetwork(const std::string &network,
                                            const std::string &fallback) {
    const char *name = network == "testnet" ? "NEAR_TESTNET_TX_RPC_URL" : "NEAR_TX_RPC_URL";
    return env(name).value_or(fallback);
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

bool oracleEnabled() {
    const auto value = env("ORACLE_ENABLED").value_or("false");
    if (value == "true" || value == "1" || value == "yes") return true;
    if (value == "false" || value == "0" || value == "no") return false;
    throw std::runtime_error("ORACLE_ENABLED must be true or false");
}

bool perpTradingEnabled() {
    const auto value = env("PERP_TRADING_ENABLED").value_or("false");
    if (value == "true" || value == "1" || value == "yes") return true;
    if (value == "false" || value == "0" || value == "no") return false;
    throw std::runtime_error("PERP_TRADING_ENABLED must be true or false");
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

Json::Value indexerStatus(
    const std::shared_ptr<afterbell::indexer::Worker> &worker) {
    Json::Value body;
    body["enabled"] = static_cast<bool>(worker);
    if (!worker) {
        body["status"] = "disabled";
        return body;
    }
    const auto state = worker->state();
    body["status"] = afterbell::indexer::Worker::stateName(state);
    body["lastIndexedBlock"] = Json::UInt64(worker->lastIndexedBlock());
    const auto lastSuccess = worker->lastSuccessfulPollAtMs();
    if (lastSuccess > 0) body["lastSuccessfulPollAtMs"] = Json::UInt64(lastSuccess);
    return body;
}

Json::Value oracleStatus(const std::shared_ptr<afterbell::oracle::Worker> &worker) {
    if (!worker) {
        Json::Value body;
        body["enabled"] = false;
        body["status"] = "disabled";
        return body;
    }
    return worker->status();
}

void logStartup(unsigned short port, const std::string &network,
                bool isIndexerEnabled, bool isOracleEnabled,
                bool isPerpTradingEnabled,
                Json::ArrayIndex marketCount) {
    const auto timestamp = std::chrono::duration_cast<std::chrono::milliseconds>(
                               std::chrono::system_clock::now().time_since_epoch())
                               .count();
    Json::Value event;
    event["event"] = "service.starting";
    event["timestamp_ms"] = static_cast<Json::Int64>(timestamp);
    event["service"] = "afterbell-api";
    event["port"] = port;
    event["network"] = network;
    event["indexer_enabled"] = isIndexerEnabled;
    event["oracle_enabled"] = isOracleEnabled;
        event["perp_trading_enabled"] = isPerpTradingEnabled;
    event["market_count"] = marketCount;
    Json::StreamWriterBuilder writer;
    writer["indentation"] = "";
    std::cerr << Json::writeString(writer, event) << '\n';
}

void logActivityBestEffort(const std::shared_ptr<afterbell::api::DashboardStore> &store,
                           const std::string &network, const std::string &accountId,
                           const std::string &eventType, const std::string &title,
                           const std::string &message, const std::string &market,
                           const std::string &side, const std::string &amount,
                           const std::string &transactionHash) {
    try { store->recordActivity(network, accountId, eventType, title, message, market, side, amount, transactionHash); }
    catch (const std::exception &error) { std::cerr << "dashboard activity persistence failed after confirmed transaction: " << error.what() << '\n'; }
}

} // namespace

int main() {
    try {
        loadEnvFile(".env");
        loadEnvFile("backend/.env");

        const auto databaseUrl = env("DATABASE_URL");
        if (!databaseUrl) throw std::runtime_error("DATABASE_URL is required");
        auto profileStore = std::make_shared<afterbell::api::ProfileStore>(
            env("WAITLIST_DATABASE_URL").value_or(*databaseUrl));
        auto dashboardStore = std::make_shared<afterbell::api::DashboardStore>(*databaseUrl);
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
        const afterbell::ai::ProviderConfig ottoConfig{
            env("NEAR_AI_API_KEY").value_or(""),
            env("NEAR_AI_MODEL").value_or(""),
            env("GROQ_API_KEY").value_or(""),
            env("GROQ_MODEL").value_or(""),
        };

        const bool runIndexer = indexerEnabled();
        const bool runOracle = oracleEnabled();
        const bool runTrading = perpTradingEnabled();
        const bool accountSetupConfigured = env("ORACLE_RELAYER_ACCOUNT_ID").has_value() &&
                                            env("ORACLE_RELAYER_PRIVATE_KEY").has_value();
        const auto nearRpcUrl = (runIndexer || runOracle || runTrading || accountSetupConfigured)
                                    ? nearRpcUrlForNetwork(*network)
                                    : std::string{};
        std::shared_ptr<afterbell::oracle::Worker> oracleWorker;
        if (runOracle) {
            auto poolRegistryPath = env("ROBINHOOD_POOL_REGISTRY_PATH");
            if (!poolRegistryPath) {
                std::ifstream fromRepoRoot("backend/rh_uniswap_pools.json");
                poolRegistryPath = fromRepoRoot ? "backend/rh_uniswap_pools.json"
                                                 : "rh_uniswap_pools.json";
            }
            afterbell::oracle::WorkerConfig oracleConfig{
                *databaseUrl,
                requiredEnv("ROBINHOOD_RPC_URL"),
                nearRpcUrl,
                nearTransactionRpcUrlForNetwork(*network, nearRpcUrl),
                *contractId,
                *network,
                requiredEnv("ORACLE_RELAYER_ACCOUNT_ID"),
                requiredEnv("ORACLE_RELAYER_PRIVATE_KEY"),
                requiredEnv("ORACLE_SIGNER_PRIVATE_KEY"),
                *registryPath,
                *poolRegistryPath,
            };
            oracleConfig.pollIntervalMs = static_cast<std::uint32_t>(
                uintEnvOr("ORACLE_POLL_INTERVAL_MS", 5'000));
            oracleConfig.twapWindowSeconds = static_cast<std::uint32_t>(
                uintEnvOr("ORACLE_TWAP_WINDOW_SECONDS", 300));
            oracleWorker = std::make_shared<afterbell::oracle::Worker>(
                std::move(oracleConfig));
            oracleWorker->start();
        }
        std::shared_ptr<afterbell::indexer::Worker> indexerWorker;
        if (runIndexer) {
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
            indexerWorker = std::make_shared<afterbell::indexer::Worker>(
                std::move(indexerConfig), nearRpcUrl,
                requiredEnv("INDEXER_TX_API_URL"), *databaseUrl,
                uintEnvOr("INDEXER_POLL_INTERVAL_MS", 5000));
            indexerWorker->start();
        }
        std::shared_ptr<afterbell::trading::Service> accountService;
        std::shared_ptr<afterbell::trading::Service> tradingService;
        if (runTrading || accountSetupConfigured) {
            const auto tradeFeeBps = uintEnvOr("PERP_TRADE_FEE_BPS", 10);
            if (tradeFeeBps > 100) throw std::runtime_error("PERP_TRADE_FEE_BPS must be between 0 and 100");
            auto service = std::make_shared<afterbell::trading::Service>(
                afterbell::trading::Config{
                    *network,
                    nearRpcUrl,
                    nearTransactionRpcUrlForNetwork(*network, nearRpcUrl),
                    *contractId,
                    env("ORACLE_RELAYER_ACCOUNT_ID").value_or(""),
                    env("ORACLE_RELAYER_PRIVATE_KEY").value_or(""),
                    env("PERP_QUOTE_SIGNER_PRIVATE_KEY").value_or(""),
                    env("OWNER_REGISTRATION_STORAGE_DEPOSIT_YOCTO").value_or("10000000000000000000000"),
                    env("AGENT_RECORD_STORAGE_CREDIT_TARGET_YOCTO").value_or("50000000000000000000000"),
                    static_cast<std::uint16_t>(tradeFeeBps),
                });
            accountService = service;
            if (runTrading) {
                if (env("PERP_QUOTE_SIGNER_PRIVATE_KEY").value_or("").empty())
                    throw std::runtime_error("PERP_QUOTE_SIGNER_PRIVATE_KEY is required when PERP_TRADING_ENABLED=true");
                tradingService = service;
                tradingService->startLimitWorker();
            }
        }

        drogon::app().setThreadNum(4);
        afterbell::registerRequestLogger();
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
            [databaseUrl = *databaseUrl, indexerWorker, oracleWorker](
                const drogon::HttpRequestPtr &,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!databaseReady(databaseUrl)) {
                    Json::Value body = errorBody("database_unavailable", "Database is unavailable");
                    body["status"] = "unavailable";
                    body["database"] = "unavailable";
                    body["indexer"] = indexerStatus(indexerWorker);
                    body["oracle"] = oracleStatus(oracleWorker);
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                Json::Value body;
                body["status"] = "ready";
                body["database"] = "ready";
                body["indexer"] = indexerStatus(indexerWorker);
                body["oracle"] = oracleStatus(oracleWorker);
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            },
            {drogon::Get});
        drogon::app().registerHandler(
            "/v1/otto/providers",
            [ottoConfig](const drogon::HttpRequestPtr &,
                         std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                callback(drogon::HttpResponse::newHttpJsonResponse(
                    afterbell::ai::providerStatus(ottoConfig)));
            },
            {drogon::Get});
        drogon::app().registerHandler(
            "/v1/otto/session/challenge",
            [accountService, contractId = *contractId](const drogon::HttpRequestPtr &request,
                         std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try {
                    if (!accountService) throw std::runtime_error("Wallet sessions are unavailable.");
                    const auto input = request->getJsonObject();
                    if (!input || !(*input)["account_id"].isString()) throw std::invalid_argument("Connect a wallet to use Otto.");
                    const auto accountId = (*input)["account_id"].asString();
                    const auto owner = accountService->ownerStatus(accountId);
                    if (!owner["registered"].asBool()) throw std::invalid_argument("Finish setting up this Afterbell account before using Otto.");
                    callback(drogon::HttpResponse::newHttpJsonResponse(afterbell::ai::createWalletChallenge(accountId, owner["owner"], contractId)));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("otto_session_challenge_failed", error.what()));
                    response->setStatusCode(drogon::k401Unauthorized); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/otto/session/verify",
            [accountService](const drogon::HttpRequestPtr &request,
                         std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try {
                    if (!accountService) throw std::runtime_error("Wallet sessions are unavailable.");
                    const auto input = request->getJsonObject();
                    if (!input || !(*input)["challenge_id"].isString() || !(*input)["signature"].isString())
                        throw std::invalid_argument("Wallet signature is required.");
                    const auto challengeId = (*input)["challenge_id"].asString();
                    const auto accountId = afterbell::ai::walletChallengeAccount(challengeId);
                    if (accountId.empty()) throw std::invalid_argument("Wallet session request expired. Please try again.");
                    const auto owner = accountService->ownerStatus(accountId);
                    if (!owner["registered"].asBool()) throw std::invalid_argument("Afterbell account owner could not be verified.");
                    Json::Value body; body["token"] = afterbell::ai::verifyWalletChallenge(challengeId, (*input)["signature"].asString(), owner["owner"]);
                    body["expires_in_seconds"] = 1800;
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("otto_session_verification_failed", error.what()));
                    response->setStatusCode(drogon::k401Unauthorized); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/otto/chat",
            [ottoConfig, tradingService](const drogon::HttpRequestPtr &request,
                         std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                const auto input = request->getJsonObject();
                std::string validationError;
                if (request->body().size() > 32 * 1024 || !input ||
                    !afterbell::ai::validChatRequest(*input, validationError)) {
                    if (request->body().size() > 32 * 1024) validationError = "Otto request is too large.";
                    Json::Value body = errorBody("otto_request_invalid",
                        input ? validationError : "Request body must be JSON.");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k400BadRequest);
                    callback(response);
                    return;
                }
                const auto authorization = request->getHeader("authorization");
                constexpr std::string_view bearer = "Bearer ";
                if (authorization.rfind(bearer, 0) != 0) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("otto_session_required", "Sign with your wallet to start a secure Otto session."));
                    response->setStatusCode(drogon::k401Unauthorized); callback(response); return;
                }
                const auto accountId = afterbell::ai::walletSessionAccount(authorization.substr(bearer.size()));
                if (accountId.empty()) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("otto_session_expired", "Your Otto wallet session expired. Please send your message again to renew it."));
                    response->setStatusCode(drogon::k401Unauthorized); callback(response); return;
                }
                if (!afterbell::ai::allowRequestForAccount(accountId)) {
                    Json::Value body = errorBody("otto_rate_limited",
                        "You’ve reached Otto’s request limit. Please wait a minute and try again.");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k429TooManyRequests);
                    callback(response);
                    return;
                }
                const auto provider = (*input)["provider"].asString();
                if ((provider == "near_ai" && (ottoConfig.nearApiKey.empty() || ottoConfig.nearModel.empty() ||
                                                ottoConfig.nearModel == "openai/privacy-filter")) ||
                    (provider == "qwen" && (ottoConfig.groqApiKey.empty() || ottoConfig.groqModel.empty()))) {
                    const auto detail = provider == "near_ai" && ottoConfig.nearModel == "openai/privacy-filter"
                        ? "NEAR_AI_MODEL must be a chat model. openai/privacy-filter is reserved for privacy redaction."
                        : "This Otto provider is not configured. Choose another provider or contact support.";
                    Json::Value body = errorBody("otto_provider_unavailable", detail);
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                Json::Value authenticatedInput = *input;
                authenticatedInput["account_id"] = accountId;
                const auto marketId = authenticatedInput["market_id"].asString();
                auto contextProvider = [tradingService, accountId, marketId]() {
                    Json::Value context(Json::objectValue);
                    if (!tradingService) { context["context_error"] = "Trading context is unavailable."; return context; }
                    auto marketFuture = std::async(std::launch::async, [tradingService, marketId] { return tradingService->marketSnapshot(marketId); });
                    auto balanceFuture = std::async(std::launch::async, [tradingService, accountId] { return tradingService->accountBalance(accountId); });
                    auto positionsFuture = std::async(std::launch::async, [tradingService, accountId] { return tradingService->listPositions(accountId); });
                    try { context["market"] = marketFuture.get(); } catch (...) { context["market_error"] = "Current market snapshot unavailable."; }
                    try { context["account"] = balanceFuture.get(); } catch (...) { context["account_error"] = "Current account balance unavailable."; }
                    try {
                        const auto positions = positionsFuture.get();
                        context["positions"] = Json::Value(Json::arrayValue);
                        for (Json::ArrayIndex i = 0; i < positions.size() && i < 10; ++i) {
                            Json::Value position(Json::objectValue);
                            for (const auto *key : {"market_id", "is_long", "notional_usdc", "margin_usdc", "leverage_bps", "entry_price", "mark_price", "mark_observed_at_ms", "unrealized_pnl_usdc"})
                                if (positions[i].isMember(key)) position[key] = positions[i][key];
                            context["positions"].append(std::move(position));
                        }
                    } catch (...) { context["positions_error"] = "Current positions unavailable."; }
                    return context;
                };
                callback(afterbell::ai::streamChat(std::move(authenticatedInput), ottoConfig, std::move(contextProvider)));
            },
            {drogon::Post});
        drogon::app().registerHandler(
            "/health/status",
            [databaseUrl = *databaseUrl, indexerWorker, oracleWorker](
                const drogon::HttpRequestPtr &,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                const bool databaseIsReady = databaseReady(databaseUrl);
                Json::Value body;
                body["status"] = databaseIsReady ? "ready" : "degraded";
                body["api"] = "running";
                body["database"] = databaseIsReady ? "ready" : "unavailable";
                body["indexer"] = indexerStatus(indexerWorker);
                body["oracle"] = oracleStatus(oracleWorker);
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            },
            {drogon::Get});
        drogon::app().registerHandler(
            "/v1/markets",
            [markets, network = *network, contractId = *contractId, runTrading](
                const drogon::HttpRequestPtr &,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                Json::Value body;
                body["network"] = network;
                body["contractId"] = contractId;
                body["markets"] = markets;
                body["tradingEnabled"] = runTrading;
                for (auto &market : body["markets"]) {
                    market["tradingEnabled"] = runTrading;
                }
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            },
            {drogon::Get});
        drogon::app().registerHandler(
            "/v1/markets/snapshot",
            [markets, tradingService](const drogon::HttpRequestPtr &, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                Json::Value body(Json::arrayValue);
                std::vector<std::future<Json::Value>> snapshots;
                snapshots.reserve(markets.size());
                for (const auto &market : markets) {
                    const auto id = market["symbol"].asString();
                    snapshots.push_back(std::async(std::launch::async, [tradingService, id] {
                        try {
                            if (!tradingService) throw std::runtime_error("Perpetual trading reads are disabled");
                            return tradingService->marketSnapshot(id);
                        } catch (const std::exception &error) {
                            Json::Value item;
                            item["market_id"] = id;
                            item["error"] = error.what();
                            return item;
                        }
                    }));
                }
                for (auto &snapshot : snapshots) body.append(snapshot.get());
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/markets/history",
            [dashboardStore, network = *network](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try {
                    const auto market = request->getParameter("market_id");
                    const auto range = request->getParameter("range");
                    const auto now = static_cast<std::uint64_t>(std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::system_clock::now().time_since_epoch()).count());
                    const std::uint64_t duration = range == "1h" ? 3'600'000ULL : range == "1d" ? 86'400'000ULL : range == "1w" ? 604'800'000ULL : range == "1m" ? 2'592'000'000ULL : range == "all" ? now : 86'400'000ULL;
                    Json::Value body; body["market_id"] = market; body["range"] = range;
                    body["points"] = dashboardStore->marketHistory(network, market, now - duration);
                    body["volume_points"] = dashboardStore->marketVolumeHistory(network, market, now - duration);
                    const auto historyStart = dashboardStore->marketHistoryStart(network, market);
                    body["history_start_ms"] = historyStart ? Json::Value(Json::UInt64(*historyStart)) : Json::Value();
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("market_history_unavailable", error.what())); response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); }
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/markets/volume",
            [markets, dashboardStore, network = *network](const drogon::HttpRequestPtr &, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                Json::Value body(Json::arrayValue);
                for (const auto &market : markets) { Json::Value item; item["market_id"] = market["symbol"]; item["volume_usdc_raw"] = dashboardStore->marketVolume24h(network, market["symbol"].asString()); body.append(item); }
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/account/balance",
            [tradingService](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Trading reads are unavailable"); callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->accountBalance(request->getParameter("account_id")))); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("balance_unavailable", error.what())); response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); }
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/account/near-balance",
            [tradingService](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Wallet balance reads are unavailable"); callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->nearWalletBalance(request->getParameter("address")))); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("wallet_balance_unavailable", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/account/deposit/instructions",
            [contractId = *contractId, network = *network, tokenId = env("NEAR_USDC_TOKEN_ID").value_or("")](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                const auto accountId = request->getParameter("account_id");
                if (accountId.empty() || accountId.size() > 128 || accountId.rfind("protocol:", 0) == 0 || accountId.rfind("agent:", 0) == 0 || tokenId.empty()) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("deposit_instructions_unavailable", "Deposit configuration is unavailable for this account")); response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                Json::Value body; body["account_id"] = accountId; body["contract_id"] = contractId; body["token_id"] = tokenId; body["network"] = network;
                callback(drogon::HttpResponse::newHttpJsonResponse(body));
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/liquidity/overview",
            [tradingService](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Liquidity actions are unavailable"); callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->liquidityOverview(request->getParameter("account_id")))); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("liquidity_unavailable", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/liquidity/deposit/payload",
            [tradingService](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Liquidity actions are unavailable"); const auto input = request->getJsonObject(); if (!input) throw std::invalid_argument("request body must be JSON"); callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->liquidityDepositPayload(*input))); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("liquidity_deposit_payload_failed", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/liquidity/deposit",
            [tradingService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Liquidity actions are unavailable"); const auto input = request->getJsonObject(); if (!input) throw std::invalid_argument("request body must be JSON"); auto body = tradingService->submitLiquidityDeposit(*input); logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "liquidity_added", "Liquidity added", "USDC was added to the shared vault.", "", "", (*input)["amount_usdc"].asString(), body["transaction_hash"].asString()); callback(drogon::HttpResponse::newHttpJsonResponse(body)); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("liquidity_deposit_failed", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/liquidity/withdrawal/payload",
            [tradingService](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Liquidity actions are unavailable"); const auto input = request->getJsonObject(); if (!input) throw std::invalid_argument("request body must be JSON"); callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->liquidityWithdrawalPayload(*input))); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("liquidity_withdrawal_payload_failed", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/liquidity/withdrawal",
            [tradingService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Liquidity actions are unavailable"); const auto input = request->getJsonObject(); if (!input) throw std::invalid_argument("request body must be JSON"); auto body = tradingService->submitLiquidityWithdrawal(*input); logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "liquidity_withdrawn", "Liquidity withdrawn", "LP shares were redeemed and USDC was returned to your Afterbell balance.", "", "", "", body["transaction_hash"].asString()); callback(drogon::HttpResponse::newHttpJsonResponse(body)); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("liquidity_withdrawal_failed", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/withdrawal/payload",
            [tradingService](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Withdrawals are unavailable"); const auto input = request->getJsonObject(); if (!input) throw std::invalid_argument("request body must be JSON"); callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->accountWithdrawalPayload(*input))); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("withdrawal_payload_failed", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/withdrawal",
            [tradingService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { if (!tradingService) throw std::runtime_error("Withdrawals are unavailable"); const auto input = request->getJsonObject(); if (!input) throw std::invalid_argument("request body must be JSON"); auto body = tradingService->submitAccountWithdrawal(*input); logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "withdrawal_submitted", "Withdrawal submitted", "USDC withdrawal to your NEAR address was submitted.", "", "", (*input)["amount_usdc"].asString(), body["transaction_hash"].asString()); callback(drogon::HttpResponse::newHttpJsonResponse(body)); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("withdrawal_failed", error.what())); response->setStatusCode(drogon::k422UnprocessableEntity); callback(response); }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/history",
            [dashboardStore, network = *network](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try { const auto account = request->getParameter("account_id"); Json::Value body; body["events"] = dashboardStore->activityHistory(network, account); callback(drogon::HttpResponse::newHttpJsonResponse(body)); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("history_unavailable", error.what())); response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); }
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/notifications",
            [dashboardStore, network = *network](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                const auto account = request->getParameter("account_id");
                try {
                    if (request->method() == drogon::Get) { dashboardStore->syncDepositNotifications(network, account); Json::Value body; body["notifications"] = dashboardStore->notifications(network, account); callback(drogon::HttpResponse::newHttpJsonResponse(body)); return; }
                    dashboardStore->clearNotifications(network, account); Json::Value body; body["cleared"] = true; callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("notifications_unavailable", error.what())); response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); }
            }, {drogon::Get, drogon::Delete});
        drogon::app().registerHandler(
            "/v1/notifications/{id}",
            [dashboardStore, network = *network](const drogon::HttpRequestPtr &request, std::function<void(const drogon::HttpResponsePtr &)> &&callback, const std::string &id) {
                try { Json::Value body; body["deleted"] = dashboardStore->deleteNotification(network, request->getParameter("account_id"), id); callback(drogon::HttpResponse::newHttpJsonResponse(body)); }
                catch (const std::exception &error) { auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("notification_delete_failed", error.what())); response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); }
            }, {drogon::Delete});
        drogon::app().registerHandler(
            "/v1/account/owner/status",
            [accountService](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!accountService) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_setup_unavailable", "Account setup is not configured"));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input || !(*input)["account_id"].isString()) throw std::invalid_argument("account_id is required");
                    callback(drogon::HttpResponse::newHttpJsonResponse(accountService->ownerStatus((*input)["account_id"].asString())));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_status_failed", error.what()));
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/owner/payload",
            [accountService](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!accountService) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_setup_unavailable", "Account setup is not configured"));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    callback(drogon::HttpResponse::newHttpJsonResponse(accountService->ownerRegistrationPayload(*input)));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("owner_payload_failed", error.what()));
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/owner/register",
            [accountService](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!accountService) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_setup_unavailable", "Account setup is not configured"));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    callback(drogon::HttpResponse::newHttpJsonResponse(accountService->registerOwner(*input)));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("owner_registration_failed", error.what()));
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/agent/payload",
            [accountService](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!accountService) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_setup_unavailable", "Account setup is not configured"));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    callback(drogon::HttpResponse::newHttpJsonResponse(accountService->agentSetupPayload(*input)));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("agent_payload_failed", error.what()));
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/agent/create",
            [accountService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!accountService) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_setup_unavailable", "Account setup is not configured"));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    auto body = accountService->createAgentRecord(*input);
                    if (body["status"] == "created") logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "agent_created",
                        "Trading agent created", "A trading agent was authorized for your Afterbell account.", "", "", "0", body["transaction_hash"].asString());
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("agent_creation_failed", error.what()));
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/username/check",
            [profileStore](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                try {
                    callback(drogon::HttpResponse::newHttpJsonResponse(profileStore->checkUsername(
                        request->getParameter("account_id"), request->getParameter("username"))));
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("username_check_unavailable", error.what()));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response);
                }
            }, {drogon::Get});
        drogon::app().registerHandler(
            "/v1/account/username/claim",
            [accountService, profileStore](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!accountService) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_setup_unavailable", "Account setup is not configured"));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    const auto accountId = (*input)["account_id"].asString();
                    if (!accountService->ownerStatus(accountId)["registered"].asBool())
                        throw std::invalid_argument("Register your connected wallet before choosing a username.");
                    callback(drogon::HttpResponse::newHttpJsonResponse(
                        profileStore->claimUsername(accountId, (*input)["username"].asString())));
                } catch (const afterbell::api::UsernameUnavailable &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("username_unavailable", error.what()));
                    response->setStatusCode(drogon::k409Conflict); callback(response);
                } catch (const std::invalid_argument &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("username_claim_failed", error.what()));
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("username_store_unavailable", error.what()));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/account/username/reclaim",
            [accountService, profileStore](const drogon::HttpRequestPtr &request,
                std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!accountService) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("account_setup_unavailable", "Account setup is not configured"));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    const auto accountId = (*input)["account_id"].asString();
                    if (!accountService->ownerStatus(accountId)["registered"].asBool())
                        throw std::invalid_argument("Register your connected wallet before reclaiming a username.");
                    callback(drogon::HttpResponse::newHttpJsonResponse(
                        profileStore->reclaimUsername(accountId, (*input)["email"].asString())));
                } catch (const afterbell::api::UsernameUnavailable &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("username_unavailable", error.what()));
                    response->setStatusCode(drogon::k409Conflict); callback(response);
                } catch (const std::invalid_argument &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("username_reclaim_failed", error.what()));
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                } catch (const std::exception &error) {
                    auto response = drogon::HttpResponse::newHttpJsonResponse(errorBody("username_store_unavailable", error.what()));
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response);
                }
            }, {drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/quote",
            [tradingService](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trade quoting is not enabled on this service");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    auto body = tradingService->createOpenQuote(*input);
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const afterbell::trading::RiskCapacityExceeded &error) {
                    Json::Value body = errorBody("risk_capacity_exceeded", error.what());
                    body["error"]["allowed_max_notional_usdc"] = error.allowedMaxRaw();
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity);
                    callback(response);
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("quote_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity);
                    callback(response);
                }
            },
            {drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/open",
            [tradingService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trade execution is not enabled on this service");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    auto body = tradingService->submitOpen(*input);
                    logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "position_opened",
                        "Position opened", (*input)["side"].asString() + " " + (*input)["market_id"].asString() + " position",
                        (*input)["market_id"].asString(), (*input)["side"].asString(), (*input)["notional_usdc"].asString(), body["transaction_hash"].asString());
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("trade_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity);
                    callback(response);
                }
            },
            {drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/positions",
            [tradingService](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trading API is disabled");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                try {
                    const auto accountId = request->getParameter("account_id");
                    Json::Value body;
                    body["account_id"] = accountId;
                    body["positions"] = tradingService->listPositions(accountId);
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("positions_unavailable", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity);
                    callback(response);
                }
            },
            {drogon::Get});
        drogon::app().registerHandler(
            "/v1/trade/close/quote",
            [tradingService](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trading API is disabled");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->createCloseQuote(*input)));
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("close_quote_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity);
                    callback(response);
                }
            },
            {drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/close",
            [tradingService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trading API is disabled");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable);
                    callback(response);
                    return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    auto body = tradingService->submitClose(*input);
                    logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "position_closed",
                        "Position closed", "Position " + (*input)["position_id"].asString() + " closed",
                        (*input)["market_id"].asString(), (*input)["side"].asString(), (*input)["notional_usdc"].asString(), body["transaction_hash"].asString());
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("close_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity);
                    callback(response);
                }
            },
            {drogon::Post});

        drogon::app().registerHandler(
            "/v1/trade/limits",
            [tradingService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trading API is disabled");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    if (request->method() == drogon::Get) {
                        Json::Value body; body["orders"] = tradingService->listLimitOrders(request->getParameter("account_id"));
                        callback(drogon::HttpResponse::newHttpJsonResponse(body)); return;
                    }
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    auto body = tradingService->submitLimitOrder(*input);
                    logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "limit_order_created",
                        "Limit order created", (*input)["side"].asString() + " limit order placed for " + (*input)["market_id"].asString(),
                        (*input)["market_id"].asString(), (*input)["side"].asString(), (*input)["notional_usdc"].asString(), body["transaction_hash"].asString());
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("limit_order_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            },
            {drogon::Get, drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/limits/quote",
            [tradingService](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trading API is disabled");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->createLimitOrderMessage(*input)));
                } catch (const afterbell::trading::RiskCapacityExceeded &error) {
                    Json::Value body = errorBody("risk_capacity_exceeded", error.what());
                    body["error"]["allowed_max_notional_usdc"] = error.allowedMaxRaw();
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("limit_quote_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            },
            {drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/limits/cancel/quote",
            [tradingService](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trading API is disabled");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->createLimitCancelMessage(*input)));
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("limit_cancel_quote_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            },
            {drogon::Post});
        drogon::app().registerHandler(
            "/v1/trade/limits/cancel",
            [tradingService, dashboardStore, network = *network](const drogon::HttpRequestPtr &request,
                             std::function<void(const drogon::HttpResponsePtr &)> &&callback) {
                if (!tradingService) {
                    Json::Value body = errorBody("trading_disabled", "Trading API is disabled");
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k503ServiceUnavailable); callback(response); return;
                }
                try {
                    const auto input = request->getJsonObject();
                    if (!input) throw std::invalid_argument("request body must be JSON");
                    auto body = tradingService->cancelLimitOrder(*input);
                    logActivityBestEffort(dashboardStore, network, (*input)["account_id"].asString(), "limit_order_cancelled",
                        "Limit order cancelled", "Limit order " + (*input)["order_id"].asString() + " cancelled",
                        (*input)["market_id"].asString(), (*input)["side"].asString(), (*input)["notional_usdc"].asString(), body["transaction_hash"].asString());
                    callback(drogon::HttpResponse::newHttpJsonResponse(body));
                } catch (const std::exception &error) {
                    Json::Value body = errorBody("limit_cancel_rejected", error.what());
                    auto response = drogon::HttpResponse::newHttpJsonResponse(body);
                    response->setStatusCode(drogon::k422UnprocessableEntity); callback(response);
                }
            },
            {drogon::Post});

        const auto port = portFromEnv();
        logStartup(port, *network, runIndexer, runOracle, runTrading, markets.size());
        std::cerr << "afterbell-api listening on port " << port << " (" << *network
                  << ", indexer " << (runIndexer ? "enabled" : "disabled") << ")\n";
        drogon::app().addListener("0.0.0.0", port).run();
        if (indexerWorker) indexerWorker->stop();
        if (oracleWorker) oracleWorker->stop();
        if (tradingService) tradingService->stopLimitWorker();
    } catch (const std::exception &error) {
        std::cerr << "afterbell-api: " << error.what() << "\n";
        return 1;
    }
}
