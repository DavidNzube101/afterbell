#include "indexer/indexer.hpp"

#include <chrono>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <optional>
#include <stdexcept>
#include <string>
#include <thread>
#include <utility>

namespace {

std::string trim(std::string value) {
    const auto first = value.find_first_not_of(" \t\r\n");
    if (first == std::string::npos) return {};
    const auto last = value.find_last_not_of(" \t\r\n");
    return value.substr(first, last - first + 1);
}

void loadEnvFile(const std::string &path) {
    std::ifstream file(path);
    std::string line;
    while (std::getline(file, line)) {
        line = trim(std::move(line));
        if (line.empty() || line.front() == '#') continue;
        if (line.rfind("export ", 0) == 0) line = trim(line.substr(7));
        const auto equals = line.find('=');
        if (equals == std::string::npos) continue;
        auto name = trim(line.substr(0, equals));
        auto value = trim(line.substr(equals + 1));
        if (name.empty() || std::getenv(name.c_str()) != nullptr) continue;
        if (value.size() >= 2 &&
            ((value.front() == '"' && value.back() == '"') ||
             (value.front() == '\'' && value.back() == '\''))) {
            value = value.substr(1, value.size() - 2);
        }
        setenv(name.c_str(), value.c_str(), 0);
    }
}

std::string requiredEnv(const char *name) {
    const auto *value = std::getenv(name);
    if (value == nullptr || *value == '\0') {
        throw std::runtime_error(std::string(name) + " is required");
    }
    return value;
}

std::optional<std::uint64_t> optionalUintEnv(const char *name) {
    const auto *value = std::getenv(name);
    if (value == nullptr || *value == '\0') return std::nullopt;
    std::size_t consumed = 0;
    const auto parsed = std::stoull(value, &consumed);
    if (consumed != std::string(value).size()) {
        throw std::runtime_error(std::string(name) + " must be an unsigned integer");
    }
    return parsed;
}

std::uint64_t uintEnvOr(const char *name, std::uint64_t fallback) {
    const auto parsed = optionalUintEnv(name);
    return parsed.value_or(fallback);
}

} // namespace

int main() {
    try {
        loadEnvFile(".env");
        auto network = requiredEnv("NEAR_NETWORK");
        if (network != "testnet" && network != "mainnet") {
            throw std::runtime_error("NEAR_NETWORK must be testnet or mainnet");
        }
        afterbell::indexer::IndexerConfig config{
            network,
            requiredEnv("A1_CONTRACT_ID"),
            requiredEnv("NEAR_USDC_TOKEN_ID"),
            optionalUintEnv("INDEXER_START_BLOCK"),
        };
        afterbell::indexer::NearRpc rpc(requiredEnv("INDEXER_NEAR_RPC_URL"));
        const auto chainId = rpc.chainId();
        if (chainId != network) {
            throw std::runtime_error("NEAR_RPC_URL is connected to " + chainId +
                                     " but NEAR_NETWORK is " + network);
        }
        afterbell::indexer::FastNearClient fastNear(requiredEnv("INDEXER_TX_API_URL"));
        afterbell::indexer::Database database(requiredEnv("DATABASE_URL"));
        config.finalityLagBlocks = uintEnvOr("INDEXER_FINALITY_LAG_BLOCKS", 1000);
        config.rescanOverlapBlocks = uintEnvOr("INDEXER_RESCAN_OVERLAP_BLOCKS", 10000);
        afterbell::indexer::Indexer indexer(std::move(config), rpc, fastNear, database);

        const auto pollMs = uintEnvOr("INDEXER_POLL_INTERVAL_MS", 5000);
        if (pollMs < 500 || config.finalityLagBlocks == 0 ||
            config.rescanOverlapBlocks == 0) {
            throw std::runtime_error("Indexer polling and overlap settings must be positive");
        }

        std::cerr << "afterbell-indexer started for " << network << "\n";
        for (;;) {
            try {
                const auto previousCursor = indexer.cursor();
                const auto indexedEvents = indexer.indexAvailableRange();
                if (indexer.cursor() > previousCursor) {
                    std::cerr << "scanned A1 history through block " << indexer.cursor()
                              << "; deposit events indexed: " << indexedEvents << "\n";
                }
                if (indexedEvents > 0) {
                    std::cerr << "indexed A1 deposit events: " << indexedEvents << "\n";
                }
            } catch (const std::exception &error) {
                std::cerr << "indexer pass failed; cursor retained for retry: "
                          << error.what() << "\n";
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(pollMs));
        }
    } catch (const std::exception &error) {
        std::cerr << "afterbell-indexer: " << error.what() << "\n";
        return 1;
    }
}
