#include <drogon/drogon.h>

#include "http/request_logger.hpp"
#include "indexer/worker.hpp"
#include "oracle/worker.hpp"
#include "trading/service.hpp"

#include <libpq-fe.h>

#include <cstdint>
#include <chrono>
#include <cstdlib>
#include <fstream>
#include <iostream>
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

std::string nearRpcUrlForNetwork(const std::string &network) {
    if (network == "testnet") return requiredEnv("NEAR_TESTNET_RPC_URL");
    if (network == "mainnet") return requiredEnv("NEAR_RPC_URL");
    throw std::runtime_error("NEAR_NETWORK must be testnet or mainnet");
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

        const bool runIndexer = indexerEnabled();
        const bool runOracle = oracleEnabled();
        const bool runTrading = perpTradingEnabled();
        const auto nearRpcUrl = (runIndexer || runOracle || runTrading)
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
                uintEnvOr("ORACLE_POLL_INTERVAL_MS", 15'000));
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
        std::shared_ptr<afterbell::trading::Service> tradingService;
        if (runTrading) {
            const auto tradeFeeBps = uintEnvOr("PERP_TRADE_FEE_BPS", 10);
            if (tradeFeeBps > 100) throw std::runtime_error("PERP_TRADE_FEE_BPS must be between 0 and 100");
            tradingService = std::make_shared<afterbell::trading::Service>(
                afterbell::trading::Config{
                    *network,
                    nearRpcUrl,
                    nearTransactionRpcUrlForNetwork(*network, nearRpcUrl),
                    *contractId,
                    requiredEnv("ORACLE_RELAYER_ACCOUNT_ID"),
                    requiredEnv("ORACLE_RELAYER_PRIVATE_KEY"),
                    requiredEnv("PERP_QUOTE_SIGNER_PRIVATE_KEY"),
                    static_cast<std::uint16_t>(tradeFeeBps),
                });
            tradingService->startLimitWorker();
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
            [tradingService](const drogon::HttpRequestPtr &request,
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
                    callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->submitClose(*input)));
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
            [tradingService](const drogon::HttpRequestPtr &request,
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
                    callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->submitLimitOrder(*input)));
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
                    callback(drogon::HttpResponse::newHttpJsonResponse(tradingService->cancelLimitOrder(*input)));
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
