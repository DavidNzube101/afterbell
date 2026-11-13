#include "trading/service.hpp"

#include "indexer/near_rpc.hpp"
#include "oracle/crypto.hpp"
#include "oracle/near_tx_lock.hpp"

#include <openssl/evp.h>
#include <openssl/rand.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cctype>
#include <cstdio>
#include <limits>
#include <memory>
#include <iostream>
#include <future>
#include <stdexcept>
#include <string_view>
#include <utility>
#include <vector>

namespace afterbell::trading {
namespace {

std::string compactJson(const Json::Value &value) {
    Json::StreamWriterBuilder builder;
    builder["indentation"] = "";
    return Json::writeString(builder, value);
}

Json::Value viewCall(afterbell::indexer::NearRpc &rpc, const std::string &contract,
                     const std::string &method, const Json::Value &args) {
    const auto text = compactJson(args);
    Json::Value request;
    request["request_type"] = "call_function";
    request["finality"] = "final";
    request["account_id"] = contract;
    request["method_name"] = method;
    request["args_base64"] = afterbell::oracle::base64Encode(
        reinterpret_cast<const std::uint8_t *>(text.data()), text.size());
    const auto result = rpc.call("query", request);
    if (!result["result"].isArray()) throw std::runtime_error("A1 view call returned no bytes");
    std::string bytes;
    for (const auto &value : result["result"]) {
        if (!value.isUInt() || value.asUInt() > 255) throw std::runtime_error("A1 view call returned invalid bytes");
        bytes.push_back(static_cast<char>(value.asUInt()));
    }
    Json::CharReaderBuilder builder;
    Json::Value output;
    std::string errors;
    auto reader = std::unique_ptr<Json::CharReader>(builder.newCharReader());
    if (!reader->parse(bytes.data(), bytes.data() + bytes.size(), &output, &errors))
        throw std::runtime_error("A1 view call returned invalid JSON");
    return output;
}

template <typename Function>
auto asyncRpcRead(const std::string &url, Function function)
    -> std::future<decltype(function(std::declval<afterbell::indexer::NearRpc &>()))> {
    return std::async(std::launch::async, [url, function = std::move(function)]() mutable {
        afterbell::indexer::NearRpc reader(url);
        return function(reader);
    });
}

std::uint64_t u64(const Json::Value &value, const char *field) {
    try {
        if (value.isString()) return std::stoull(value.asString());
        if (value.isUInt64()) return value.asUInt64();
        if (value.isInt64() && value.asInt64() >= 0) return static_cast<std::uint64_t>(value.asInt64());
    } catch (...) {}
    throw std::invalid_argument(std::string("invalid ") + field);
}

unsigned __int128 u128(const Json::Value &value, const char *field) {
    if (!value.isString() && !value.isUInt64() && !value.isInt64()) throw std::invalid_argument(std::string("invalid ") + field);
    const auto text = value.isString() ? value.asString()
                     : value.isUInt64() ? std::to_string(value.asUInt64())
                                        : std::to_string(value.asInt64());
    if (text.empty() || text.front() == '-') throw std::invalid_argument(std::string("invalid ") + field);
    unsigned __int128 parsed = 0;
    for (const auto c : text) {
        if (!std::isdigit(static_cast<unsigned char>(c))) throw std::invalid_argument(std::string("invalid ") + field);
        const auto digit = static_cast<unsigned>(c - '0');
        if (parsed > (static_cast<unsigned __int128>(-1) - digit) / 10) throw std::invalid_argument(std::string(field) + " is too large");
        parsed = parsed * 10 + digit;
    }
    return parsed;
}

std::string decimal(unsigned __int128 value) {
    if (!value) return "0";
    std::string out;
    while (value) { out.push_back(static_cast<char>('0' + value % 10)); value /= 10; }
    std::reverse(out.begin(), out.end());
    return out;
}

std::uint64_t blockTimeMs(afterbell::indexer::NearRpc &rpc) {
    Json::Value params; params["finality"] = "final";
    const auto block = rpc.call("block", params);
    return u64(block["header"]["timestamp_nanosec"], "NEAR block timestamp") / 1'000'000;
}

std::uint64_t tradingNonce(afterbell::indexer::NearRpc &rpc,
                           const std::string &contract, const std::string &account) {
    static constexpr char prefix[] = "afterbell:a1:perp:quote_nonce:";
    std::vector<std::uint8_t> key(prefix, prefix + sizeof(prefix) - 1);
    const auto length = static_cast<std::uint32_t>(account.size());
    for (unsigned i = 0; i < 4; ++i) key.push_back(static_cast<std::uint8_t>(length >> (8 * i)));
    key.insert(key.end(), account.begin(), account.end());
    Json::Value request;
    request["request_type"] = "view_state";
    request["finality"] = "final";
    request["account_id"] = contract;
    request["prefix_base64"] = afterbell::oracle::base64Encode(key.data(), key.size());
    const auto state = rpc.call("query", request);
    if (!state["values"].isArray() || state["values"].empty()) return 0;
    const auto raw = afterbell::oracle::base64Decode(state["values"][0]["value"].asString());
    if (raw.size() != 8) throw std::runtime_error("A1 trading nonce state is malformed");
    std::uint64_t nonce = 0;
    for (unsigned i = 0; i < 8; ++i) nonce |= std::uint64_t(raw[i]) << (8 * i);
    return nonce;
}

unsigned __int128 stateU128(afterbell::indexer::NearRpc &rpc, const std::string &contract,
                            const std::string &keyText) {
    const auto *begin = reinterpret_cast<const std::uint8_t *>(keyText.data());
    Json::Value query; query["request_type"] = "view_state"; query["finality"] = "final";
    query["account_id"] = contract; query["prefix_base64"] = afterbell::oracle::base64Encode(begin, keyText.size());
    const auto result = rpc.call("query", query);
    if (!result["values"].isArray() || result["values"].empty()) return 0;
    const auto bytes = afterbell::oracle::base64Decode(result["values"][0]["value"].asString());
    if (bytes.size() != 16) throw std::runtime_error("A1 liquidity share state is malformed");
    unsigned __int128 value = 0;
    for (unsigned i = 0; i < 16; ++i) value |= static_cast<unsigned __int128>(bytes[i]) << (8 * i);
    return value;
}

std::string usdcDisplay(unsigned __int128 raw) {
    const auto whole = raw / 1'000'000;
    const auto fraction = static_cast<unsigned>(raw % 1'000'000);
    char decimals[7];
    std::snprintf(decimals, sizeof(decimals), "%06u", fraction);
    return decimal(whole) + "." + decimals;
}

Json::Value orderCapacity(afterbell::indexer::NearRpc &rpc,
                         const std::string &contract,
                         const std::string &account,
                         const std::string &marketId,
                         bool isLong) {
    Json::Value args;
    args["account_id"] = account;
    args["market_id"] = marketId;
    args["is_long"] = isLong;
    const auto capacity = viewCall(rpc, contract, "perp_order_capacity", args);
    if (!capacity.isObject() || !capacity.isMember("allowed_max_notional_usdc"))
        throw std::runtime_error("A1 returned invalid order capacity data");
    return capacity;
}

void validateUserAccount(const std::string &account) {
    if (account.empty() || account.size() > 128 || account.rfind("protocol:", 0) == 0 || account.rfind("agent:", 0) == 0)
        throw std::invalid_argument("invalid user account_id");
}

std::uint64_t actionExpiry(afterbell::indexer::NearRpc &rpc) {
    return blockTimeMs(rpc) + 60'000;
}

void appendU32(std::vector<std::uint8_t> &out, std::uint32_t value) {
    for (unsigned i = 0; i < 4; ++i) out.push_back(static_cast<std::uint8_t>(value >> (8 * i)));
}
void appendU64(std::vector<std::uint8_t> &out, std::uint64_t value) {
    for (unsigned i = 0; i < 8; ++i) out.push_back(static_cast<std::uint8_t>(value >> (8 * i)));
}
void appendString(std::vector<std::uint8_t> &out, const std::string &value) {
    appendU32(out, static_cast<std::uint32_t>(value.size()));
    out.insert(out.end(), value.begin(), value.end());
}
void appendBytes(std::vector<std::uint8_t> &out, const std::vector<std::uint8_t> &value) {
    appendU32(out, static_cast<std::uint32_t>(value.size()));
    out.insert(out.end(), value.begin(), value.end());
}
std::vector<std::uint8_t> nearKey(const std::string &key) {
    if (key.rfind("ed25519:", 0) != 0) throw std::invalid_argument("trade relayer key must use ed25519 format");
    auto decoded = afterbell::oracle::base58Decode(key.substr(8));
    if (decoded.size() != 64) throw std::invalid_argument("trade relayer keypair must contain 64 bytes");
    return decoded;
}
std::array<std::uint8_t, 32> nearPublic(const std::vector<std::uint8_t> &key) {
    using Pkey = std::unique_ptr<EVP_PKEY, decltype(&EVP_PKEY_free)>;
    Pkey pkey(EVP_PKEY_new_raw_private_key(EVP_PKEY_ED25519, nullptr, key.data(), 32), EVP_PKEY_free);
    std::array<std::uint8_t, 32> pub{};
    std::size_t size = pub.size();
    if (!pkey || EVP_PKEY_get_raw_public_key(pkey.get(), pub.data(), &size) != 1 || size != pub.size()) throw std::runtime_error("could not derive trade relayer public key");
    if (!std::equal(pub.begin(), pub.end(), key.begin() + 32)) throw std::invalid_argument("trade relayer keypair public key mismatch");
    return pub;
}
std::vector<std::uint8_t> nearSign(const std::vector<std::uint8_t> &key,
                                  const std::uint8_t *data, std::size_t size) {
    using Pkey = std::unique_ptr<EVP_PKEY, decltype(&EVP_PKEY_free)>;
    using Ctx = std::unique_ptr<EVP_MD_CTX, decltype(&EVP_MD_CTX_free)>;
    Pkey pkey(EVP_PKEY_new_raw_private_key(EVP_PKEY_ED25519, nullptr, key.data(), 32), EVP_PKEY_free);
    Ctx ctx(EVP_MD_CTX_new(), EVP_MD_CTX_free);
    std::vector<std::uint8_t> signature(64);
    std::size_t length = signature.size();
    if (!pkey || !ctx || EVP_DigestSignInit(ctx.get(), nullptr, nullptr, nullptr, pkey.get()) != 1 || EVP_DigestSign(ctx.get(), signature.data(), &length, data, size) != 1 || length != signature.size()) throw std::runtime_error("trade relayer transaction signing failed");
    return signature;
}

void appendU128(std::vector<std::uint8_t> &out, unsigned __int128 value) {
    for (unsigned i = 0; i < 16; ++i) out.push_back(static_cast<std::uint8_t>(value >> (8 * i)));
}

void validateAccountOwnerBinding(const std::string &accountId, const Json::Value &owner) {
    if (owner.isMember("Ethereum") && owner["Ethereum"]["address"].isArray() &&
        owner["Ethereum"]["address"].size() == 20) {
        std::string address = "0x";
        static constexpr char hex[] = "0123456789abcdef";
        for (const auto &byte : owner["Ethereum"]["address"]) {
            if (!byte.isUInt() || byte.asUInt() > 255) throw std::invalid_argument("invalid Ethereum owner address");
            address.push_back(hex[byte.asUInt() >> 4]); address.push_back(hex[byte.asUInt() & 15]);
        }
        if (accountId != "ethereum:" + address) throw std::invalid_argument("account_id does not match the Ethereum wallet address");
        return;
    }
    if (owner.isMember("SolanaEd25519") && owner["SolanaEd25519"]["public_key"].isArray() &&
        owner["SolanaEd25519"]["public_key"].size() == 32) {
        std::vector<std::uint8_t> key;
        for (const auto &byte : owner["SolanaEd25519"]["public_key"]) {
            if (!byte.isUInt() || byte.asUInt() > 255) throw std::invalid_argument("invalid Solana owner key");
            key.push_back(static_cast<std::uint8_t>(byte.asUInt()));
        }
        const auto expected = "solana:" + afterbell::oracle::base58Encode(key.data(), key.size());
        if (accountId != expected) throw std::invalid_argument("account_id does not match the Solana wallet key");
        return;
    }
    if (owner.isMember("NearEd25519") && owner["NearEd25519"]["public_key"].isArray() &&
        owner["NearEd25519"]["public_key"].size() == 32) {
        for (const auto &byte : owner["NearEd25519"]["public_key"])
            if (!byte.isUInt() || byte.asUInt() > 255) throw std::invalid_argument("invalid NEAR owner key");
        if (accountId.rfind("near:", 0) != 0 || accountId.size() <= 5)
            throw std::invalid_argument("NEAR account_id must use the near:<account> namespace");
        return;
    }
    throw std::invalid_argument("owner must contain one supported A1 wallet key");
}

Json::Value relayCall(const Config &config, const std::string &method, const Json::Value &args,
                      unsigned __int128 attachedDeposit = 0) {
    const auto key = nearKey(config.relayerPrivateKey);
    const auto pub = nearPublic(key);
    const auto publicKeyText = "ed25519:" + afterbell::oracle::base58Encode(pub.data(), pub.size());
    afterbell::indexer::NearRpc txRpc(config.transactionRpcUrl);
    if (txRpc.chainId() != config.network) throw std::runtime_error("trade transaction RPC is on the wrong NEAR network");
    Json::Value accessArgs; accessArgs["request_type"] = "view_access_key"; accessArgs["finality"] = "final";
    accessArgs["account_id"] = config.relayerAccountId; accessArgs["public_key"] = publicKeyText;
    const auto nonce = u64(txRpc.call("query", accessArgs)["nonce"], "relayer nonce") + 1;
    Json::Value blockArgs; blockArgs["finality"] = "final";
    const auto block = txRpc.call("block", blockArgs);
    const auto blockHash = afterbell::oracle::base58Decode(block["header"]["hash"].asString());
    if (blockHash.size() != 32) throw std::runtime_error("NEAR returned an invalid block hash");
    const auto argsText = compactJson(args);
    std::vector<std::uint8_t> tx;
    appendString(tx, config.relayerAccountId); tx.push_back(0); tx.insert(tx.end(), pub.begin(), pub.end());
    appendU64(tx, nonce); appendString(tx, config.contractId); tx.insert(tx.end(), blockHash.begin(), blockHash.end());
    appendU32(tx, 1); tx.push_back(2); appendString(tx, method);
    appendBytes(tx, std::vector<std::uint8_t>(argsText.begin(), argsText.end()));
    appendU64(tx, 180'000'000'000'000ULL); appendU128(tx, attachedDeposit);
    const auto hash = afterbell::oracle::sha256(tx.data(), tx.size());
    const auto signature = nearSign(key, hash.data(), hash.size());
    tx.push_back(0); tx.insert(tx.end(), signature.begin(), signature.end());
    Json::Value params(Json::arrayValue); params.append(afterbell::oracle::base64Encode(tx.data(), tx.size()));
    const auto result = txRpc.call("broadcast_tx_commit", params);
    if (result["status"].isMember("Failure") || result["final_execution_status"] == "FAILURE") {
        const auto executionError = result["status"]["Failure"]["ActionError"]["kind"]["FunctionCallError"]["ExecutionError"];
        const auto transactionHash = result["transaction"]["hash"].asString();
        auto message = "A1 rejected the " + method + " transaction";
        if (executionError.isString() && !executionError.asString().empty()) message += ": " + executionError.asString();
        if (!transactionHash.empty()) message += " (transaction " + transactionHash + ")";
        throw std::runtime_error(message);
    }
    Json::Value response; response["status"] = "finalized";
    response["transaction_hash"] = result["transaction"]["hash"];
    return response;
}

Json::Value checkedQuote(afterbell::indexer::NearRpc &rpc, const Config &config,
                         const Json::Value &input, bool create) {
    const auto account = input["account_id"].asString();
    const auto marketId = input["market_id"].asString();
    if (account.empty() || account.size() > 64 || marketId.empty() || marketId.size() > 16) throw std::invalid_argument("invalid account_id or market_id");
    const bool isLong = input["side"].asString() == "long";
    if (!isLong && input["side"].asString() != "short") throw std::invalid_argument("side must be long or short");
    const auto notional = u128(input["notional_usdc"], "notional_usdc");
    const auto leverageRaw = u64(input["leverage_bps"], "leverage_bps");
    const auto slippageRaw = u64(input["max_slippage_bps"], "max_slippage_bps");
    if (leverageRaw > std::numeric_limits<std::uint32_t>::max() || slippageRaw > 1'000 || notional == 0 || leverageRaw < 10'000)
        throw std::invalid_argument("order size, leverage, or slippage is outside the allowed range");
    const auto leverage = static_cast<std::uint32_t>(leverageRaw);
    const auto slippage = static_cast<std::uint16_t>(slippageRaw);

    const auto capacity = orderCapacity(rpc, config.contractId, account, marketId, isLong);
    const auto allowedNotional = u128(capacity["allowed_max_notional_usdc"], "allowed order capacity");
    if (leverage > u64(capacity["max_leverage_bps"], "market max leverage") ||
        slippage > u64(capacity["max_execution_deviation_bps"], "market slippage cap"))
        throw std::invalid_argument("order exceeds the market risk limits");
    if (notional > allowedNotional) throw RiskCapacityExceeded(decimal(allowedNotional));
    const auto sequence = u64(capacity["price_sequence"], "oracle sequence");
    const auto referencePrice = u128(capacity["mark_price_e8"], "oracle mark price");
    const auto chainNow = u64(capacity["now_ms"], "NEAR block timestamp");
    const auto expiry = chainNow + 30'000;

    auto ownerRead = asyncRpcRead(config.readRpcUrl, [contract = config.contractId, account](auto &reader) {
        Json::Value args; args["account_id"] = account;
        return viewCall(reader, contract, "owner_of", args);
    });
    const auto margin = (notional * 10'000 + leverage - 1) / leverage;
    const auto fee = (notional * config.feeBps + 9'999) / 10'000;
    auto nonceRead = asyncRpcRead(config.readRpcUrl, [contract = config.contractId, account](auto &reader) {
        return tradingNonce(reader, contract, account) + 1;
    });
    auto balanceRead = asyncRpcRead(config.readRpcUrl, [contract = config.contractId, account](auto &reader) {
        Json::Value args; args["account_id"] = account;
        return viewCall(reader, contract, "balance_of", args);
    });
    const auto nonce = nonceRead.get();
    const auto balance = u128(balanceRead.get(), "USDC balance");
    if (ownerRead.get().isNull()) throw std::invalid_argument("account has no registered owner");
    if (balance < margin + fee) throw std::runtime_error("insufficient free USDC for margin and fee");

    std::string positionId;
    if (create) {
        std::array<std::uint8_t, 12> random{};
        if (RAND_bytes(random.data(), random.size()) != 1) throw std::runtime_error("could not generate position id");
        static constexpr char hex[] = "0123456789abcdef";
        positionId = "p_";
        for (const auto byte : random) { positionId.push_back(hex[byte >> 4]); positionId.push_back(hex[byte & 15]); }
    } else {
        positionId = input["position_id"].asString();
        if (positionId.empty() || positionId.size() > 64) throw std::invalid_argument("invalid position_id");
        if (u64(input["nonce"], "nonce") != nonce || u64(input["price_sequence"], "price_sequence") != sequence ||
            u128(input["reference_price"], "reference_price") != referencePrice || u64(input["expires_at_ms"], "expires_at_ms") <= chainNow ||
            u64(input["expires_at_ms"], "expires_at_ms") > chainNow + 60'000 ||
            u64(input["fee_bps"], "fee_bps") != config.feeBps ||
            u64(input["leverage_bps"], "leverage_bps") != leverage ||
            u64(input["max_slippage_bps"], "max_slippage_bps") != slippage) throw std::runtime_error("quote is stale or altered; request a new quote");
    }

    Json::Value quoteArgs;
    quoteArgs["account_id"] = account; quoteArgs["position_id"] = positionId;
    quoteArgs["market_id"] = marketId; quoteArgs["is_long"] = isLong;
    quoteArgs["notional"] = decimal(notional); quoteArgs["leverage_bps"] = leverage;
    quoteArgs["reference_price"] = decimal(referencePrice); quoteArgs["max_slippage_bps"] = slippage;
    quoteArgs["price_sequence"] = Json::UInt64(sequence); quoteArgs["fee_bps"] = config.feeBps;
    quoteArgs["nonce"] = Json::UInt64(nonce); quoteArgs["expires_at_ms"] = Json::UInt64(create ? expiry : u64(input["expires_at_ms"], "expires_at_ms"));
    auto messageRead = asyncRpcRead(config.readRpcUrl, [contract = config.contractId, quoteArgs](auto &reader) {
        return viewCall(reader, contract, "trade_quote_message", quoteArgs);
    });
    auto signerRead = asyncRpcRead(config.readRpcUrl, [contract = config.contractId](auto &reader) {
        return viewCall(reader, contract, "perp_quote_signer", Json::Value(Json::objectValue));
    });
    const auto messageValue = messageRead.get();
    if (!messageValue.isString()) throw std::runtime_error("A1 did not return a trade authorization message");

    Json::Value signer = signerRead.get();
    const auto expectedAddress = afterbell::oracle::ethereumAddress(config.quoteSignerPrivateKey);
    const auto &addressBytes = signer["Ethereum"]["address"];
    if (!addressBytes.isArray() || addressBytes.size() != 20) throw std::runtime_error("A1 quote signer is not configured as the backend EVM signer");
    std::string actualAddress = "0x";
    static constexpr char hex[] = "0123456789abcdef";
    for (const auto &b : addressBytes) {
        if (!b.isUInt() || b.asUInt() > 255) throw std::runtime_error("A1 quote signer configuration is malformed");
        actualAddress.push_back(hex[b.asUInt() >> 4]); actualAddress.push_back(hex[b.asUInt() & 15]);
    }
    std::string normalized = expectedAddress;
    std::transform(normalized.begin(), normalized.end(), normalized.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
    if (actualAddress != normalized) throw std::runtime_error("A1 quote signer does not match PERP_QUOTE_SIGNER_PRIVATE_KEY");
    const auto signature = afterbell::oracle::eip191Sign(messageValue.asString(), config.quoteSignerPrivateKey);
    Json::Value quote;
    quote["account_id"] = account; quote["position_id"] = positionId;
    quote["market_id"] = marketId; quote["side"] = isLong ? "long" : "short";
    quote["notional_usdc"] = decimal(notional); quote["leverage_bps"] = leverage;
    quote["reference_price"] = decimal(referencePrice); quote["max_slippage_bps"] = slippage;
    quote["price_sequence"] = Json::UInt64(sequence); quote["fee_bps"] = config.feeBps;
    quote["nonce"] = Json::UInt64(nonce); quote["expires_at_ms"] = quoteArgs["expires_at_ms"];
    quote["message"] = messageValue.asString();
    quote["quote_signature"] = afterbell::oracle::base64Encode(signature.data(), signature.size());
    quote["required_margin_usdc"] = decimal(margin); quote["fee_usdc"] = decimal(fee);
    quote["free_balance_usdc"] = decimal(balance);
    quote["allowed_max_notional_usdc"] = decimal(allowedNotional);
    return quote;
}

} // namespace

RiskCapacityExceeded::RiskCapacityExceeded(std::string allowedMaxRaw)
    : std::runtime_error("Order size exceeds current risk capacity. Maximum currently allowed order size is $" +
                         usdcDisplay(u128(Json::Value(allowedMaxRaw), "allowed maximum")) + " USDC."),
      allowedMaxRaw_(std::move(allowedMaxRaw)) {}

Service::Service(Config config) : config_(std::move(config)) {
    if (config_.network != "testnet" && config_.network != "mainnet") throw std::invalid_argument("NEAR_NETWORK must be testnet or mainnet");
    if (config_.contractId.empty() || config_.readRpcUrl.empty() || config_.transactionRpcUrl.empty() || config_.relayerAccountId.empty()) throw std::invalid_argument("trading RPC, contract, and relayer configuration are required");
    (void)nearPublic(nearKey(config_.relayerPrivateKey));
    if (!config_.quoteSignerPrivateKey.empty()) (void)afterbell::oracle::ethereumAddress(config_.quoteSignerPrivateKey);
    if (config_.feeBps > 100) throw std::invalid_argument("PERP_TRADE_FEE_BPS must be between 0 and 100");
}

Service::~Service() { stopLimitWorker(); }

Json::Value Service::ownerStatus(const std::string &accountId) const {
    if (accountId.empty() || accountId.size() > 128) throw std::invalid_argument("invalid account_id");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("account read RPC is on the wrong NEAR network");
    Json::Value args; args["account_id"] = accountId;
    const auto owner = viewCall(rpc, config_.contractId, "owner_of", args);
    Json::Value response; response["account_id"] = accountId;
    response["registered"] = !owner.isNull();
    if (!owner.isNull()) response["owner"] = owner;
    return response;
}

Json::Value Service::marketSnapshot(const std::string &marketId) const {
    if (marketId.empty() || marketId.size() > 16) throw std::invalid_argument("invalid market_id");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("market read RPC is on the wrong NEAR network");
    Json::Value args; args["market_id"] = marketId;
    const auto config = viewCall(rpc, config_.contractId, "perp_market", args);
    const auto price = viewCall(rpc, config_.contractId, "market_price", args);
    const auto health = viewCall(rpc, config_.contractId, "oracle_health", args);
    const auto exposure = viewCall(rpc, config_.contractId, "perp_exposure", args);
    const auto hourlyFunding = viewCall(rpc, config_.contractId, "perp_funding_rate_bps_per_hour", args);
    Json::Value result;
    result["market_id"] = marketId;
    result["enabled"] = !config.isNull() && config["enabled"].asBool();
    result["close_only"] = !config.isNull() && config["close_only"].asBool();
    result["mark_price_e8"] = price.isNull() ? Json::Value() : price["price"];
    result["observed_at_ms"] = price.isNull() ? Json::Value() : price["observed_at_ms"];
    result["sequence"] = price.isNull() ? Json::Value() : price["sequence"];
    result["confidence_bps"] = health["confidence_bps"];
    result["spot_twap_deviation_bps"] = health["spot_twap_deviation_bps"];
    result["circuit_broken"] = health["circuit_broken"];
    const auto longOpen = exposure.isNull() ? 0 : u128(exposure["long_notional_usdc"], "long open interest");
    const auto shortOpen = exposure.isNull() ? 0 : u128(exposure["short_notional_usdc"], "short open interest");
    result["long_open_notional_usdc"] = decimal(longOpen);
    result["short_open_notional_usdc"] = decimal(shortOpen);
    result["open_interest_usdc"] = decimal(longOpen + shortOpen);
    const auto funding = hourlyFunding.isUInt() ? hourlyFunding.asUInt() : hourlyFunding.asUInt64();
    result["funding_rate_bps_hourly"] = Json::UInt64(funding);
    result["funding_rate_bps_8h"] = Json::UInt64(funding * 8);
    if (!config.isNull()) {
        result["max_leverage_bps"] = config["max_leverage_bps"];
        result["max_position_usdc"] = config["max_position_usdc"];
        result["max_execution_deviation_bps"] = config["max_execution_deviation_bps"];
    }
    return result;
}

Json::Value Service::accountBalance(const std::string &accountId) const {
    if (accountId.empty() || accountId.size() > 128) throw std::invalid_argument("invalid account_id");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("balance read RPC is on the wrong NEAR network");
    Json::Value args; args["account_id"] = accountId;
    Json::Value result; result["account_id"] = accountId;
    result["balance_usdc"] = viewCall(rpc, config_.contractId, "balance_of", args);
    return result;
}

Json::Value Service::nearWalletBalance(const std::string &address) const {
    if (address.empty() || address.size() > 64 || address.find_first_of(" \t\r\n") != std::string::npos)
        throw std::invalid_argument("invalid NEAR wallet address");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("wallet balance RPC is on the wrong NEAR network");
    Json::Value params; params["request_type"] = "view_account"; params["finality"] = "final"; params["account_id"] = address;
    const auto account = rpc.call("query", params);
    Json::Value result; result["address"] = address; result["network"] = config_.network;
    result["amount_yocto"] = account["amount"]; result["locked_yocto"] = account["locked"];
    return result;
}

Json::Value Service::liquidityOverview(const std::string &accountId) const {
    validateUserAccount(accountId);
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("liquidity read RPC is on the wrong NEAR network");
    Json::Value args; args["account_id"] = accountId;
    const auto owner = viewCall(rpc, config_.contractId, "owner_of", args);
    if (owner.isNull()) throw std::invalid_argument("account owner is not registered");
    Json::Value result; result["account_id"] = accountId;
    result["balance_usdc"] = viewCall(rpc, config_.contractId, "balance_of", args);
    result["lp_shares"] = viewCall(rpc, config_.contractId, "lp_shares_of", args);
    result["vault_nav_usdc"] = viewCall(rpc, config_.contractId, "vault_nav_usdc", Json::Value(Json::objectValue));
    result["available_liquidity_usdc"] = viewCall(rpc, config_.contractId, "vault_available_payout", Json::Value(Json::objectValue));
    result["vault_balance_usdc"] = viewCall(rpc, config_.contractId, "vault_balance", Json::Value(Json::objectValue));
    result["total_lp_shares"] = decimal(stateU128(rpc, config_.contractId, "afterbell:a1:lp:total_shares"));
    return result;
}

Json::Value Service::liquidityDepositPayload(const Json::Value &request) const {
    if (!request["account_id"].isString()) throw std::invalid_argument("account_id is required");
    const auto account = request["account_id"].asString(); validateUserAccount(account);
    const auto amount = u128(request["amount_usdc"], "amount_usdc");
    if (amount == 0) throw std::invalid_argument("deposit amount must be positive");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("liquidity read RPC is on the wrong NEAR network");
    Json::Value args; args["account_id"] = account;
    if (viewCall(rpc, config_.contractId, "owner_of", args).isNull()) throw std::invalid_argument("account owner is not registered");
    const auto nonce = tradingNonce(rpc, config_.contractId, account) + 1;
    const auto expiry = actionExpiry(rpc);
    Json::Value response; response["account_id"] = account; response["amount_usdc"] = decimal(amount);
    response["nonce"] = Json::UInt64(nonce); response["expires_at_ms"] = Json::UInt64(expiry);
    response["recipient"] = config_.contractId;
    response["message"] = "afterbell:v1\ncontract:" + config_.contractId + "\naction:deposit_vault\naccount:" + account + "\namount:" + decimal(amount) + "\nnonce:" + std::to_string(nonce) + "\nexpires_at_ms:" + std::to_string(expiry);
    return response;
}

Json::Value Service::submitLiquidityDeposit(const Json::Value &request) {
    if (!request["account_id"].isString() || !request["signature"].isString()) throw std::invalid_argument("account_id and signature are required");
    const auto account = request["account_id"].asString(); validateUserAccount(account);
    const auto amount = u128(request["amount_usdc"], "amount_usdc");
    const auto nonce = u64(request["nonce"], "nonce");
    const auto expiry = u64(request["expires_at_ms"], "expires_at_ms");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("liquidity read RPC is on the wrong NEAR network");
    if (nonce != tradingNonce(rpc, config_.contractId, account) + 1 || expiry <= blockTimeMs(rpc) || expiry > blockTimeMs(rpc) + 120'000)
        throw std::runtime_error("liquidity authorization expired or is stale; request a new signature");
    const auto signature = afterbell::oracle::base64Decode(request["signature"].asString());
    if (signature.size() != 64 && signature.size() != 65) throw std::invalid_argument("owner signature has an invalid length");
    Json::Value args; args["account_id"] = account; args["amount"] = decimal(amount);
    args["nonce"] = Json::UInt64(nonce); args["expires_at_ms"] = Json::UInt64(expiry); args["signature"] = request["signature"];
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    auto result = relayCall(config_, "deposit_vault", args);
    result["account_id"] = account; result["amount_usdc"] = decimal(amount);
    return result;
}

Json::Value Service::liquidityWithdrawalPayload(const Json::Value &request) const {
    if (!request["account_id"].isString()) throw std::invalid_argument("account_id is required");
    const auto account = request["account_id"].asString(); validateUserAccount(account);
    const auto shares = u128(request["shares"], "shares");
    const auto minAmount = u128(request["min_amount_usdc"], "min_amount_usdc");
    if (shares == 0) throw std::invalid_argument("withdrawal shares must be positive");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("liquidity read RPC is on the wrong NEAR network");
    Json::Value args; args["account_id"] = account;
    if (viewCall(rpc, config_.contractId, "owner_of", args).isNull()) throw std::invalid_argument("account owner is not registered");
    const auto nonce = tradingNonce(rpc, config_.contractId, account) + 1;
    const auto expiry = actionExpiry(rpc);
    Json::Value response; response["account_id"] = account; response["shares"] = decimal(shares);
    response["min_amount_usdc"] = decimal(minAmount); response["nonce"] = Json::UInt64(nonce);
    response["expires_at_ms"] = Json::UInt64(expiry); response["recipient"] = config_.contractId;
    response["message"] = "afterbell:v1\ncontract:" + config_.contractId + "\naction:request_vault_withdrawal\naccount:" + account + "\namount:" + decimal(shares) + ":" + decimal(minAmount) + "\nnonce:" + std::to_string(nonce) + "\nexpires_at_ms:" + std::to_string(expiry);
    return response;
}

Json::Value Service::submitLiquidityWithdrawal(const Json::Value &request) {
    if (!request["account_id"].isString() || !request["signature"].isString()) throw std::invalid_argument("account_id and signature are required");
    const auto account = request["account_id"].asString(); validateUserAccount(account);
    const auto shares = u128(request["shares"], "shares");
    const auto minAmount = u128(request["min_amount_usdc"], "min_amount_usdc");
    const auto nonce = u64(request["nonce"], "nonce"); const auto expiry = u64(request["expires_at_ms"], "expires_at_ms");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    const auto chainNow = blockTimeMs(rpc);
    if (rpc.chainId() != config_.network || nonce != tradingNonce(rpc, config_.contractId, account) + 1 || expiry <= chainNow || expiry > chainNow + 120'000)
        throw std::runtime_error("liquidity authorization expired or is stale; request a new signature");
    const auto signature = afterbell::oracle::base64Decode(request["signature"].asString());
    if (signature.size() != 64 && signature.size() != 65) throw std::invalid_argument("owner signature has an invalid length");
    Json::Value args; args["account_id"] = account; args["shares"] = decimal(shares); args["min_amount_usdc"] = decimal(minAmount);
    args["nonce"] = Json::UInt64(nonce); args["expires_at_ms"] = Json::UInt64(expiry); args["signature"] = request["signature"];
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    auto result = relayCall(config_, "request_vault_withdrawal", args); result["account_id"] = account;
    return result;
}

Json::Value Service::accountWithdrawalPayload(const Json::Value &request) const {
    if (!request["account_id"].isString() || !request["receiver_id"].isString()) throw std::invalid_argument("account_id and receiver_id are required");
    const auto account = request["account_id"].asString(); validateUserAccount(account);
    const auto amount = u128(request["amount_usdc"], "amount_usdc");
    if (amount == 0) throw std::invalid_argument("withdrawal amount must be positive");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    Json::Value args; args["account_id"] = account; args["receiver_id"] = request["receiver_id"];
    args["amount"] = decimal(amount); args["expires_at_ms"] = Json::UInt64(actionExpiry(rpc)); args["max_fee"] = "0";
    auto payload = viewCall(rpc, config_.contractId, "withdrawal_signing_payload", args);
    Json::Value nonceArgs; nonceArgs["account_id"] = account;
    const auto nonce = viewCall(rpc, config_.contractId, "withdrawal_nonce", nonceArgs).asUInt64() + 1;
    payload["action_nonce"] = Json::UInt64(nonce); payload["amount_usdc"] = decimal(amount);
    payload["account_id"] = account; payload["receiver_id"] = request["receiver_id"];
    payload["expires_at_ms"] = args["expires_at_ms"];
    return payload;
}

Json::Value Service::submitAccountWithdrawal(const Json::Value &request) {
    for (const auto *field : {"account_id", "receiver_id", "signature"})
        if (!request[field].isString() || request[field].asString().empty()) throw std::invalid_argument(std::string(field) + " is required");
    const auto account = request["account_id"].asString(); validateUserAccount(account);
    const auto amount = u128(request["amount_usdc"], "amount_usdc");
    const auto nonce = u64(request["action_nonce"], "action_nonce");
    const auto expiry = u64(request["expires_at_ms"], "expires_at_ms");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    const auto chainNow = blockTimeMs(rpc);
    Json::Value nonceArgs; nonceArgs["account_id"] = account;
    const auto previousNonce = viewCall(rpc, config_.contractId, "withdrawal_nonce", nonceArgs).asUInt64();
    if (rpc.chainId() != config_.network || nonce != previousNonce + 1 || expiry <= chainNow || expiry > chainNow + 120'000)
        throw std::runtime_error("withdrawal authorization expired or is stale; request a new signature");
    const auto signature = afterbell::oracle::base64Decode(request["signature"].asString());
    if (signature.size() != 64 && signature.size() != 65) throw std::invalid_argument("owner signature has an invalid length");
    Json::Value args; args["account_id"] = account; args["receiver_id"] = request["receiver_id"];
    args["amount"] = decimal(amount); args["nonce"] = Json::UInt64(nonce); args["expires_at_ms"] = Json::UInt64(expiry);
    args["max_fee"] = "0"; args["signature"] = request["signature"];
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    auto result = relayCall(config_, "withdraw", args); result["account_id"] = account;
    return result;
}

Json::Value Service::ownerRegistrationPayload(const Json::Value &request) const {
    if (!request["account_id"].isString() || !request["owner"].isObject()) throw std::invalid_argument("account_id and owner are required");
    const auto accountId = request["account_id"].asString();
    if (accountId.empty() || accountId.size() > 128 || accountId.rfind("protocol:", 0) == 0 || accountId.rfind("agent:", 0) == 0)
        throw std::invalid_argument("invalid or reserved account_id");
    validateAccountOwnerBinding(accountId, request["owner"]);
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("account read RPC is on the wrong NEAR network");
    constexpr std::uint64_t ownerRegistrationTtlMs = 4 * 60 * 1000;
    const auto expiry = blockTimeMs(rpc) + ownerRegistrationTtlMs;
    Json::Value args; args["account_id"] = accountId; args["owner"] = request["owner"];
    args["expires_at_ms"] = Json::UInt64(expiry);
    const auto value = viewCall(rpc, config_.contractId, "owner_registration_signing_payload", args);
    Json::Value response; response["payload"] = value; response["account_id"] = accountId;
    response["expires_at_ms"] = Json::UInt64(expiry);
    return response;
}

Json::Value Service::registerOwner(const Json::Value &request) {
    if (!request["account_id"].isString() || !request["owner"].isObject() || !request["signature"].isString())
        throw std::invalid_argument("account_id, owner, and signature are required");
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    const auto accountId = request["account_id"].asString();
    validateAccountOwnerBinding(accountId, request["owner"]);
    const auto current = ownerStatus(accountId);
    if (current["registered"].asBool()) {
        if (current["owner"] != request["owner"]) throw std::invalid_argument("account is already registered to a different wallet key");
        Json::Value response; response["account_id"] = accountId; response["status"] = "already_registered";
        return response;
    }
    const auto expiry = u64(request["expires_at_ms"], "expires_at_ms");
    const auto signature = afterbell::oracle::base64Decode(request["signature"].asString());
    if (signature.size() != 64 && signature.size() != 65) throw std::invalid_argument("owner signature has an invalid length");
    if (config_.ownerRegistrationDepositYocto.empty()) throw std::runtime_error("owner registration sponsorship is not configured");
    const auto storageCreditTarget = u128(Json::Value(config_.ownerRegistrationDepositYocto), "owner registration storage credit target");
    if (storageCreditTarget == 0 || storageCreditTarget > static_cast<unsigned __int128>(1'000'000'000'000'000'000ULL) * 10'000)
        throw std::runtime_error("owner registration storage credit target must be between 0 and 0.01 NEAR");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("account read RPC is on the wrong NEAR network");
    Json::Value storageArgs; storageArgs["account_id"] = accountId;
    const auto currentCredit = u128(
        viewCall(rpc, config_.contractId, "storage_credit_of", storageArgs),
        "owner registration storage credit");
    const auto topUp = currentCredit >= storageCreditTarget ? 0 : storageCreditTarget - currentCredit;
    Json::Value storage;
    if (topUp > 0) storage = relayCall(config_, "storage_deposit_for", storageArgs, topUp);
    Json::Value registerArgs; registerArgs["account_id"] = accountId; registerArgs["owner"] = request["owner"];
    registerArgs["expires_at_ms"] = Json::UInt64(expiry); registerArgs["signature"] = request["signature"];
    const auto registration = relayCall(config_, "register_owner", registerArgs);
    Json::Value response; response["account_id"] = accountId; response["status"] = "registered";
    if (topUp > 0) response["storage_transaction_hash"] = storage["transaction_hash"];
    response["storage_top_up_yocto"] = decimal(topUp);
    response["registration_transaction_hash"] = registration["transaction_hash"];
    return response;
}

namespace {

Json::Value agentSetupArgs(const Json::Value &request, std::uint64_t expiresAtMs) {
    for (const auto *field : {"account_id", "agent_record_id"}) {
        if (!request[field].isString() || request[field].asString().empty())
            throw std::invalid_argument(std::string(field) + " is required");
    }
    if (!request["agent_key"].isObject() || !request["agent_key"].isMember("NearEd25519") ||
        !request["agent_key"]["NearEd25519"]["public_key"].isArray() ||
        request["agent_key"]["NearEd25519"]["public_key"].size() != 32)
        throw std::invalid_argument("agent_key must contain a 32-byte Ed25519 public key");
    if (!request["allowed_markets"].isArray() || request["allowed_markets"].empty())
        throw std::invalid_argument("choose at least one agent market");
    if (!request["max_position_usdc"].isString() || !request["max_open_notional_usdc"].isString() ||
        !request["max_allocation_usdc"].isString() || !request["max_daily_loss_usdc"].isString() ||
        !request["max_leverage_bps"].isUInt())
        throw std::invalid_argument("agent limits are incomplete");

    (void)u128(request["max_position_usdc"], "max_position_usdc");
    (void)u128(request["max_open_notional_usdc"], "max_open_notional_usdc");
    (void)u128(request["max_allocation_usdc"], "max_allocation_usdc");
    (void)u128(request["max_daily_loss_usdc"], "max_daily_loss_usdc");

    Json::Value args;
    args["user_record_id"] = request["account_id"];
    args["agent_record_id"] = request["agent_record_id"];
    args["agent_key"] = request["agent_key"];
    args["allowed_markets"] = request["allowed_markets"];
    args["max_position_usdc"] = request["max_position_usdc"];
    args["max_open_notional_usdc"] = request["max_open_notional_usdc"];
    args["max_leverage_bps"] = request["max_leverage_bps"];
    args["max_allocation_usdc"] = request["max_allocation_usdc"];
    args["max_daily_loss_usdc"] = request["max_daily_loss_usdc"];
    args["expires_at_ms"] = Json::UInt64(expiresAtMs);
    return args;
}

} // namespace

Json::Value Service::agentSetupPayload(const Json::Value &request) const {
    if (!request["access_duration_days"].isUInt())
        throw std::invalid_argument("access_duration_days is required");
    const auto durationDays = request["access_duration_days"].asUInt();
    if (durationDays == 0 || durationDays > 30)
        throw std::invalid_argument("agent access duration must be between 1 and 30 days");
    const auto status = ownerStatus(request["account_id"].asString());
    if (!status["registered"].asBool()) throw std::invalid_argument("register the wallet account before creating an agent");

    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("agent setup read RPC is on the wrong NEAR network");
    const auto expiresAtMs = blockTimeMs(rpc) + static_cast<std::uint64_t>(durationDays) * 24 * 60 * 60 * 1000;
    auto args = agentSetupArgs(request, expiresAtMs);
    Json::Value nonceArgs; nonceArgs["user_record_id"] = request["account_id"];
    const auto nonce = u64(viewCall(rpc, config_.contractId, "agent_admin_nonce", nonceArgs), "agent admin nonce") + 1;
    const auto payload = viewCall(rpc, config_.contractId, "agent_setup_signing_payload", args);
    Json::Value response;
    response["payload"] = payload;
    response["expires_at_ms"] = Json::UInt64(expiresAtMs);
    response["nonce"] = Json::UInt64(nonce);
    response["agent_record_id"] = request["agent_record_id"];
    return response;
}

Json::Value Service::createAgentRecord(const Json::Value &request) {
    if (!request["signature"].isString() || !request["expires_at_ms"].isUInt64() || !request["nonce"].isUInt64())
        throw std::invalid_argument("agent signature, expiry, and nonce are required");
    const auto accountId = request["account_id"].asString();
    const auto owner = ownerStatus(accountId);
    if (!owner["registered"].asBool()) throw std::invalid_argument("register the wallet account before creating an agent");
    const auto signature = afterbell::oracle::base64Decode(request["signature"].asString());
    if (signature.size() != 64 && signature.size() != 65)
        throw std::invalid_argument("owner signature has an invalid length");

    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("agent setup read RPC is on the wrong NEAR network");
    Json::Value recordArgs; recordArgs["user_record_id"] = accountId;
    const auto existingRecord = viewCall(rpc, config_.contractId, "agent_record_of", recordArgs);
    if (existingRecord.isString() && existingRecord.asString() == request["agent_record_id"].asString()) {
        Json::Value response;
        response["account_id"] = accountId;
        response["agent_record_id"] = request["agent_record_id"];
        response["status"] = "already_created";
        return response;
    }
    auto args = agentSetupArgs(request, u64(request["expires_at_ms"], "expires_at_ms"));
    args["nonce"] = Json::UInt64(u64(request["nonce"], "nonce"));
    args["signature"] = request["signature"];
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    if (config_.agentRecordStorageCreditTargetYocto.empty())
        throw std::runtime_error("A1 record storage sponsorship is not configured");
    const auto storageCreditTarget = u128(Json::Value(config_.agentRecordStorageCreditTargetYocto), "agent record storage credit target");
    if (storageCreditTarget == 0 || storageCreditTarget > static_cast<unsigned __int128>(1'000'000'000'000'000'000ULL) * 50'000)
        throw std::runtime_error("agent record storage credit target must be between 0 and 0.05 NEAR");
    Json::Value storageArgs; storageArgs["account_id"] = accountId;
    const auto currentCredit = u128(
        viewCall(rpc, config_.contractId, "storage_credit_of", storageArgs),
        "agent record storage credit");
    const auto topUp = currentCredit >= storageCreditTarget ? 0 : storageCreditTarget - currentCredit;
    Json::Value storage;
    if (topUp > 0) storage = relayCall(config_, "storage_deposit_for", storageArgs, topUp);
    const auto result = relayCall(config_, "create_agent_record", args);
    Json::Value response;
    response["account_id"] = accountId;
    response["agent_record_id"] = request["agent_record_id"];
    response["status"] = "created";
    response["transaction_hash"] = result["transaction_hash"];
    response["storage_top_up_yocto"] = decimal(topUp);
    if (topUp > 0) response["storage_transaction_hash"] = storage["transaction_hash"];
    return response;
}

Json::Value Service::createOpenQuote(const Json::Value &request) const {
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    return checkedQuote(rpc, config_, request, true);
}

Json::Value Service::submitOpen(const Json::Value &request) {
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    const bool limitExecution = request["is_limit_order"].asBool();
    if (!request["owner_signature"].isString()) throw std::invalid_argument("owner_signature is required");
    const auto ownerSignature = afterbell::oracle::base64Decode(request["owner_signature"].asString());
    if ((!limitExecution && ownerSignature.size() != 64 && ownerSignature.size() != 65) ||
        (limitExecution && !ownerSignature.empty())) throw std::invalid_argument("owner signature has an invalid length");
    auto quote = request;
    quote.removeMember("owner_signature"); quote.removeMember("quote_signature"); quote.removeMember("message");
    afterbell::indexer::NearRpc readRpc(config_.readRpcUrl);
    if (readRpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    const auto verified = checkedQuote(readRpc, config_, quote, false);
    if (verified["message"].asString() != request["message"].asString()) throw std::runtime_error("trade message does not match quote fields");

    if (limitExecution) {
        Json::Value orderArgs; orderArgs["account_id"] = request["account_id"]; orderArgs["order_id"] = request["position_id"];
        const auto order = viewCall(readRpc, config_.contractId, "perp_limit_order", orderArgs);
        if (order.isNull() || order["market_id"] != request["market_id"] ||
            order["notional_usdc"] != request["notional_usdc"] ||
            order["leverage_bps"] != request["leverage_bps"] ||
            order["is_long"].asBool() != (request["side"].asString() == "long"))
            throw std::runtime_error("standing limit order no longer matches the execution request");
    }

    Json::Value args;
    args["account_id"] = request["account_id"]; args["position_id"] = request["position_id"];
    args["market_id"] = request["market_id"]; args["is_long"] = request["side"].asString() == "long";
    args["notional"] = request["notional_usdc"]; args["leverage_bps"] = request["leverage_bps"];
    args["reference_price"] = request["reference_price"]; args["max_slippage_bps"] = request["max_slippage_bps"];
    args["price_sequence"] = request["price_sequence"]; args["fee_bps"] = request["fee_bps"];
    args["nonce"] = request["nonce"]; args["expires_at_ms"] = request["expires_at_ms"];
    args["owner_signature"] = request["owner_signature"];
    args["quote_signature"] = verified["quote_signature"];
    const auto argsText = compactJson(args);

    const auto key = nearKey(config_.relayerPrivateKey);
    const auto pub = nearPublic(key);
    const auto publicKeyText = "ed25519:" + afterbell::oracle::base58Encode(pub.data(), pub.size());
    afterbell::indexer::NearRpc txRpc(config_.transactionRpcUrl);
    if (txRpc.chainId() != config_.network) throw std::runtime_error("trade transaction RPC is on the wrong NEAR network");
    Json::Value accessArgs; accessArgs["request_type"] = "view_access_key"; accessArgs["finality"] = "final";
    accessArgs["account_id"] = config_.relayerAccountId; accessArgs["public_key"] = publicKeyText;
    const auto access = txRpc.call("query", accessArgs);
    const auto nonce = u64(access["nonce"], "relayer nonce") + 1;
    Json::Value blockArgs; blockArgs["finality"] = "final";
    const auto block = txRpc.call("block", blockArgs);
    const auto blockHash = afterbell::oracle::base58Decode(block["header"]["hash"].asString());
    if (blockHash.size() != 32) throw std::runtime_error("NEAR returned an invalid block hash");
    std::vector<std::uint8_t> tx;
    appendString(tx, config_.relayerAccountId); tx.push_back(0); tx.insert(tx.end(), pub.begin(), pub.end());
    appendU64(tx, nonce); appendString(tx, config_.contractId); tx.insert(tx.end(), blockHash.begin(), blockHash.end());
    appendU32(tx, 1); tx.push_back(2); appendString(tx, "open_perp_position");
    appendBytes(tx, std::vector<std::uint8_t>(argsText.begin(), argsText.end()));
    appendU64(tx, 180'000'000'000'000ULL); tx.insert(tx.end(), 16, 0);
    const auto hash = afterbell::oracle::sha256(tx.data(), tx.size());
    const auto signature = nearSign(key, hash.data(), hash.size());
    tx.push_back(0); tx.insert(tx.end(), signature.begin(), signature.end());
    Json::Value params(Json::arrayValue); params.append(afterbell::oracle::base64Encode(tx.data(), tx.size()));
    const auto result = txRpc.call("broadcast_tx_commit", params);
    if (result["status"].isMember("Failure") || result["final_execution_status"] == "FAILURE") throw std::runtime_error("A1 rejected the trade transaction");
    Json::Value response; response["status"] = "finalized";
    response["transaction_hash"] = result["transaction"]["hash"];
    response["position_id"] = request["position_id"];
    return response;
}

Json::Value Service::listPositions(const std::string &accountId) const {
    if (accountId.empty() || accountId.size() > 64) throw std::invalid_argument("invalid account_id");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    static constexpr char prefix[] = "afterbell:a1:perp:position:";
    std::vector<std::uint8_t> key(prefix, prefix + sizeof(prefix) - 1);
    const auto length = static_cast<std::uint32_t>(accountId.size());
    for (unsigned i = 0; i < 4; ++i) key.push_back(static_cast<std::uint8_t>(length >> (8 * i)));
    key.insert(key.end(), accountId.begin(), accountId.end());
    Json::Value params;
    params["request_type"] = "view_state"; params["finality"] = "final";
    params["account_id"] = config_.contractId;
    params["prefix_base64"] = afterbell::oracle::base64Encode(key.data(), key.size());
    params["limit"] = 100;
    const auto state = rpc.call("query", params);
    Json::Value result(Json::arrayValue);
    if (!state["values"].isArray()) return result;
    for (const auto &entry : state["values"]) {
        const auto bytes = afterbell::oracle::base64Decode(entry["value"].asString());
        Json::CharReaderBuilder builder;
        Json::Value position;
        std::string errors;
        auto reader = std::unique_ptr<Json::CharReader>(builder.newCharReader());
        const std::string text(bytes.begin(), bytes.end());
        if (!reader->parse(text.data(), text.data() + text.size(), &position, &errors) ||
            position["account_id"].asString() != accountId) continue;
        Json::Value marketArgs; marketArgs["market_id"] = position["market_id"];
        const auto price = viewCall(rpc, config_.contractId, "market_price", marketArgs);
        if (!price.isNull()) {
            Json::Value pnlArgs;
            pnlArgs["notional_usdc"] = position["notional_usdc"];
            pnlArgs["entry_price"] = position["entry_price"];
            pnlArgs["mark_price"] = price["price"];
            pnlArgs["is_long"] = position["is_long"];
            position["mark_price"] = price["price"];
            position["mark_observed_at_ms"] = price["observed_at_ms"];
            position["unrealized_pnl_usdc"] = viewCall(rpc, config_.contractId, "calculate_linear_pnl", pnlArgs);
        }
        result.append(std::move(position));
    }
    return result;
}

Json::Value Service::createCloseQuote(const Json::Value &request) const {
    const auto account = request["account_id"].asString();
    const auto positionId = request["position_id"].asString();
    const auto slippageRaw = u64(request["max_slippage_bps"], "max_slippage_bps");
    if (account.empty() || account.size() > 64 || positionId.empty() || positionId.size() > 64 || slippageRaw > 1'000)
        throw std::invalid_argument("invalid account, position, or slippage");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    Json::Value args; args["account_id"] = account; args["position_id"] = positionId;
    const auto position = viewCall(rpc, config_.contractId, "perp_position", args);
    if (position.isNull()) throw std::invalid_argument("position does not exist");
    args.clear(); args["account_id"] = account;
    if (viewCall(rpc, config_.contractId, "owner_of", args).isNull()) throw std::invalid_argument("account has no registered owner");
    const auto marketId = position["market_id"].asString();
    args.clear(); args["market_id"] = marketId;
    const auto price = viewCall(rpc, config_.contractId, "market_price", args);
    const auto health = viewCall(rpc, config_.contractId, "oracle_health", args);
    if (price.isNull()) throw std::runtime_error("market oracle price is unavailable");
    const auto now = blockTimeMs(rpc);
    const auto observed = u64(price["observed_at_ms"], "oracle timestamp");
    if (observed > now || now - observed > 60'000 || health["circuit_broken"].asBool()) throw std::runtime_error("market oracle is stale or unsafe");
    const auto sequence = u64(price["sequence"], "oracle sequence");
    const auto nonce = tradingNonce(rpc, config_.contractId, account) + 1;
    const auto expiry = now + 30'000;
    Json::Value messageArgs;
    messageArgs["account_id"] = account; messageArgs["position_id"] = positionId;
    messageArgs["reference_price"] = price["price"];
    messageArgs["max_slippage_bps"] = static_cast<Json::UInt>(slippageRaw);
    messageArgs["price_sequence"] = Json::UInt64(sequence); messageArgs["fee_bps"] = config_.feeBps;
    messageArgs["nonce"] = Json::UInt64(nonce); messageArgs["expires_at_ms"] = Json::UInt64(expiry);
    const auto message = viewCall(rpc, config_.contractId, "close_perp_position_message", messageArgs);
    if (!message.isString()) throw std::runtime_error("A1 did not return a close authorization message");
    const auto signer = viewCall(rpc, config_.contractId, "perp_quote_signer", Json::Value(Json::objectValue));
    const auto &addressBytes = signer["Ethereum"]["address"];
    if (!addressBytes.isArray() || addressBytes.size() != 20) throw std::runtime_error("A1 quote signer is not configured as the backend EVM signer");
    const auto expectedAddress = afterbell::oracle::ethereumAddress(config_.quoteSignerPrivateKey);
    static constexpr char hex[] = "0123456789abcdef";
    std::string actualAddress = "0x";
    for (const auto &b : addressBytes) {
        if (!b.isUInt() || b.asUInt() > 255) throw std::runtime_error("A1 quote signer configuration is malformed");
        actualAddress.push_back(hex[b.asUInt() >> 4]); actualAddress.push_back(hex[b.asUInt() & 15]);
    }
    std::string normalized = expectedAddress;
    std::transform(normalized.begin(), normalized.end(), normalized.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
    if (actualAddress != normalized) throw std::runtime_error("A1 quote signer does not match PERP_QUOTE_SIGNER_PRIVATE_KEY");
    const auto signature = afterbell::oracle::eip191Sign(message.asString(), config_.quoteSignerPrivateKey);
    Json::Value quote;
    quote["account_id"] = account; quote["position_id"] = positionId;
    quote["market_id"] = marketId; quote["reference_price"] = price["price"];
    quote["max_slippage_bps"] = static_cast<Json::UInt>(slippageRaw);
    quote["price_sequence"] = Json::UInt64(sequence); quote["fee_bps"] = config_.feeBps;
    quote["nonce"] = Json::UInt64(nonce); quote["expires_at_ms"] = Json::UInt64(expiry);
    quote["message"] = message; quote["quote_signature"] = afterbell::oracle::base64Encode(signature.data(), signature.size());
    return quote;
}

Json::Value Service::submitClose(const Json::Value &request) {
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    if (!request["owner_signature"].isString()) throw std::invalid_argument("owner_signature is required");
    const auto ownerSignature = afterbell::oracle::base64Decode(request["owner_signature"].asString());
    if (ownerSignature.size() != 64 && ownerSignature.size() != 65) throw std::invalid_argument("owner signature has an invalid length");
    const auto account = request["account_id"].asString();
    const auto positionId = request["position_id"].asString();
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    Json::Value args; args["account_id"] = account; args["position_id"] = positionId;
    const auto position = viewCall(rpc, config_.contractId, "perp_position", args);
    if (position.isNull()) throw std::invalid_argument("position does not exist");
    const auto marketId = position["market_id"].asString();
    args.clear(); args["market_id"] = marketId;
    const auto price = viewCall(rpc, config_.contractId, "market_price", args);
    const auto health = viewCall(rpc, config_.contractId, "oracle_health", args);
    const auto now = blockTimeMs(rpc);
    if (price.isNull() || health["circuit_broken"].asBool()) throw std::runtime_error("market oracle is unavailable or unsafe");
    const auto sequence = u64(price["sequence"], "oracle sequence");
    const auto expiry = u64(request["expires_at_ms"], "expires_at_ms");
    const auto nonce = tradingNonce(rpc, config_.contractId, account) + 1;
    if (u64(price["observed_at_ms"], "oracle timestamp") > now || now - u64(price["observed_at_ms"], "oracle timestamp") > 60'000 ||
        u64(request["price_sequence"], "price_sequence") != sequence || u128(request["reference_price"], "reference_price") != u128(price["price"], "price") ||
        u64(request["nonce"], "nonce") != nonce || expiry <= now || expiry > now + 60'000 ||
        u64(request["fee_bps"], "fee_bps") != config_.feeBps ||
        u64(request["max_slippage_bps"], "max_slippage_bps") > 1'000) throw std::runtime_error("close quote is stale or altered; request a fresh quote");
    args.clear(); args["account_id"] = account; args["position_id"] = positionId;
    args["reference_price"] = request["reference_price"]; args["max_slippage_bps"] = request["max_slippage_bps"];
    args["price_sequence"] = request["price_sequence"]; args["fee_bps"] = request["fee_bps"];
    args["nonce"] = request["nonce"]; args["expires_at_ms"] = request["expires_at_ms"];
    const auto message = viewCall(rpc, config_.contractId, "close_perp_position_message", args);
    if (!message.isString() || message != request["message"]) throw std::runtime_error("close message does not match current position");
    const auto quoteSignature = afterbell::oracle::eip191Sign(message.asString(), config_.quoteSignerPrivateKey);
    Json::Value callArgs;
    callArgs["account_id"] = account; callArgs["position_id"] = positionId;
    callArgs["reference_price"] = request["reference_price"]; callArgs["max_slippage_bps"] = request["max_slippage_bps"];
    callArgs["price_sequence"] = request["price_sequence"]; callArgs["fee_bps"] = request["fee_bps"];
    callArgs["nonce"] = request["nonce"]; callArgs["expires_at_ms"] = request["expires_at_ms"];
    callArgs["owner_signature"] = request["owner_signature"];
    callArgs["quote_signature"] = afterbell::oracle::base64Encode(quoteSignature.data(), quoteSignature.size());
    const auto argsText = compactJson(callArgs);
    const auto key = nearKey(config_.relayerPrivateKey); const auto pub = nearPublic(key);
    const auto publicKeyText = "ed25519:" + afterbell::oracle::base58Encode(pub.data(), pub.size());
    afterbell::indexer::NearRpc txRpc(config_.transactionRpcUrl);
    if (txRpc.chainId() != config_.network) throw std::runtime_error("trade transaction RPC is on the wrong NEAR network");
    Json::Value accessArgs; accessArgs["request_type"] = "view_access_key"; accessArgs["finality"] = "final";
    accessArgs["account_id"] = config_.relayerAccountId; accessArgs["public_key"] = publicKeyText;
    const auto nonceTx = u64(txRpc.call("query", accessArgs)["nonce"], "relayer nonce") + 1;
    Json::Value blockArgs; blockArgs["finality"] = "final";
    const auto block = txRpc.call("block", blockArgs);
    const auto blockHash = afterbell::oracle::base58Decode(block["header"]["hash"].asString());
    if (blockHash.size() != 32) throw std::runtime_error("NEAR returned an invalid block hash");
    std::vector<std::uint8_t> tx; appendString(tx, config_.relayerAccountId); tx.push_back(0);
    tx.insert(tx.end(), pub.begin(), pub.end()); appendU64(tx, nonceTx); appendString(tx, config_.contractId);
    tx.insert(tx.end(), blockHash.begin(), blockHash.end()); appendU32(tx, 1); tx.push_back(2);
    appendString(tx, "close_perp_position"); appendBytes(tx, std::vector<std::uint8_t>(argsText.begin(), argsText.end()));
    appendU64(tx, 180'000'000'000'000ULL); tx.insert(tx.end(), 16, 0);
    const auto txHash = afterbell::oracle::sha256(tx.data(), tx.size());
    const auto txSignature = nearSign(key, txHash.data(), txHash.size()); tx.push_back(0);
    tx.insert(tx.end(), txSignature.begin(), txSignature.end());
    Json::Value txParams(Json::arrayValue); txParams.append(afterbell::oracle::base64Encode(tx.data(), tx.size()));
    const auto result = txRpc.call("broadcast_tx_commit", txParams);
    if (result["status"].isMember("Failure") || result["final_execution_status"] == "FAILURE") throw std::runtime_error("A1 rejected the close transaction");
    Json::Value response; response["status"] = "finalized";
    response["transaction_hash"] = result["transaction"]["hash"]; response["position_id"] = positionId;
    return response;
}

Json::Value Service::listLimitOrders(const std::string &accountId) const {
    if (accountId.empty() || accountId.size() > 64) throw std::invalid_argument("invalid account_id");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    const auto ids = viewCall(rpc, config_.contractId, "perp_limit_order_ids", Json::Value(Json::objectValue));
    Json::Value output(Json::arrayValue);
    if (!ids.isArray()) throw std::runtime_error("A1 returned invalid limit order ids");
    for (const auto &id : ids) {
        if (!id.isString()) continue;
        const auto separator = id.asString().find('\n');
        if (separator == std::string::npos || id.asString().substr(0, separator) != accountId) continue;
        Json::Value args; args["account_id"] = accountId; args["order_id"] = id.asString().substr(separator + 1);
        auto order = viewCall(rpc, config_.contractId, "perp_limit_order", args);
        if (!order.isNull()) output.append(std::move(order));
    }
    return output;
}

Json::Value Service::createLimitOrderMessage(const Json::Value &request) const {
    const auto account = request["account_id"].asString();
    const auto orderId = request["order_id"].asString();
    const auto market = request["market_id"].asString();
    const auto side = request["side"].asString();
    const auto notional = u128(request["notional_usdc"], "notional_usdc");
    const auto leverage = u64(request["leverage_bps"], "leverage_bps");
    const auto limitPrice = u128(request["limit_price"], "limit_price");
    const auto expiry = u64(request["expires_at_ms"], "expires_at_ms");
    if (account.empty() || account.size() > 64 || orderId.empty() || orderId.size() > 64 ||
        market.empty() || market.size() > 16 || (side != "long" && side != "short") ||
        leverage > std::numeric_limits<std::uint32_t>::max() || !notional || !limitPrice)
        throw std::invalid_argument("invalid limit order fields");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    const auto capacity = orderCapacity(rpc, config_.contractId, account, market, side == "long");
    const auto allowedNotional = u128(capacity["allowed_max_notional_usdc"], "allowed order capacity");
    if (leverage > u64(capacity["max_leverage_bps"], "market max leverage"))
        throw std::invalid_argument("order exceeds the market leverage limit");
    if (notional > allowedNotional) throw RiskCapacityExceeded(decimal(allowedNotional));
    const auto nonce = tradingNonce(rpc, config_.contractId, account) + 1;
    Json::Value args;
    args["account_id"] = account; args["order_id"] = orderId; args["market_id"] = market;
    args["is_long"] = side == "long"; args["notional"] = decimal(notional);
    args["leverage_bps"] = static_cast<Json::UInt>(leverage); args["limit_price"] = decimal(limitPrice);
    args["nonce"] = Json::UInt64(nonce); args["expires_at_ms"] = Json::UInt64(expiry);
    const auto message = viewCall(rpc, config_.contractId, "create_perp_limit_order_message", args);
    if (!message.isString()) throw std::runtime_error("A1 did not return a limit order signing message");
    Json::Value result = args;
    result.removeMember("is_long"); result["side"] = side;
    result["notional_usdc"] = decimal(notional); result.removeMember("notional");
    result["limit_price"] = decimal(limitPrice); result["message"] = message;
    return result;
}

Json::Value Service::submitLimitOrder(const Json::Value &request) {
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    if (!request["owner_signature"].isString()) throw std::invalid_argument("owner_signature is required");
    const auto signature = afterbell::oracle::base64Decode(request["owner_signature"].asString());
    if (signature.size() != 64 && signature.size() != 65)
        throw std::invalid_argument("owner signature has an invalid length");
    Json::Value canonical = createLimitOrderMessage(request);
    if (canonical["message"] != request["message"] || canonical["nonce"] != request["nonce"])
        throw std::runtime_error("limit order authorization is stale or altered");
    Json::Value args;
    args["account_id"] = canonical["account_id"]; args["order_id"] = canonical["order_id"];
    args["market_id"] = canonical["market_id"]; args["is_long"] = canonical["side"].asString() == "long";
    args["notional"] = canonical["notional_usdc"]; args["leverage_bps"] = canonical["leverage_bps"];
    args["limit_price"] = canonical["limit_price"]; args["nonce"] = canonical["nonce"];
    args["expires_at_ms"] = canonical["expires_at_ms"]; args["owner_signature"] = request["owner_signature"];
    auto result = relayCall(config_, "create_perp_limit_order", args);
    result["order_id"] = canonical["order_id"];
    return result;
}

Json::Value Service::createLimitCancelMessage(const Json::Value &request) const {
    const auto account = request["account_id"].asString();
    const auto orderId = request["order_id"].asString();
    if (account.empty() || account.size() > 64 || orderId.empty() || orderId.size() > 64)
        throw std::invalid_argument("invalid account_id or order_id");
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    const auto nonce = tradingNonce(rpc, config_.contractId, account) + 1;
    const auto expiry = blockTimeMs(rpc) + 30'000;
    Json::Value args; args["account_id"] = account; args["order_id"] = orderId;
    args["nonce"] = Json::UInt64(request.isMember("nonce") ? u64(request["nonce"], "nonce") : nonce);
    args["expires_at_ms"] = Json::UInt64(request.isMember("expires_at_ms") ? u64(request["expires_at_ms"], "expires_at_ms") : expiry);
    const auto message = viewCall(rpc, config_.contractId, "cancel_perp_limit_order_message", args);
    Json::Value result = args; result["message"] = message;
    return result;
}

Json::Value Service::cancelLimitOrder(const Json::Value &request) {
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    auto canonical = createLimitCancelMessage(request);
    if (!request["owner_signature"].isString() || canonical["message"] != request["message"] || canonical["nonce"] != request["nonce"])
        throw std::runtime_error("limit cancellation authorization is stale or altered");
    Json::Value args; args["account_id"] = canonical["account_id"]; args["order_id"] = canonical["order_id"];
    args["nonce"] = canonical["nonce"]; args["expires_at_ms"] = canonical["expires_at_ms"];
    args["owner_signature"] = request["owner_signature"];
    auto result = relayCall(config_, "cancel_perp_limit_order", args);
    result["order_id"] = canonical["order_id"];
    return result;
}

void Service::startLimitWorker() {
    bool expected = false;
    if (!limitWorkerRunning_.compare_exchange_strong(expected, true)) return;
    limitWorker_ = std::thread([this] {
        while (limitWorkerRunning_.load()) {
            try {
                afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
                if (rpc.chainId() == config_.network) {
                    const auto ids = viewCall(rpc, config_.contractId, "perp_limit_order_ids", Json::Value(Json::objectValue));
                    if (ids.isArray()) for (const auto &id : ids) {
                        if (!limitWorkerRunning_.load() || !id.isString()) break;
                        const auto split = id.asString().find('\n');
                        if (split == std::string::npos) continue;
                        const auto account = id.asString().substr(0, split);
                        const auto orderId = id.asString().substr(split + 1);
                        Json::Value args; args["account_id"] = account; args["order_id"] = orderId;
                        const auto order = viewCall(rpc, config_.contractId, "perp_limit_order", args);
                        if (order.isNull()) continue;
                        if (u64(order["expires_at_ms"], "order expiry") < blockTimeMs(rpc)) {
                            Json::Value cleanup; cleanup["account_id"] = account; cleanup["order_id"] = orderId;
                            std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
                            relayCall(config_, "expire_perp_limit_order", cleanup);
                            continue;
                        }
                        Json::Value marketArgs; marketArgs["market_id"] = order["market_id"];
                        const auto price = viewCall(rpc, config_.contractId, "market_price", marketArgs);
                        if (price.isNull()) continue;
                        const auto mark = u128(price["price"], "mark price");
                        const auto limit = u128(order["limit_price"], "limit price");
                        const bool isLong = order["is_long"].asBool();
                        if ((isLong && mark > limit) || (!isLong && mark < limit)) continue;
                        Json::Value trade; trade["account_id"] = account; trade["position_id"] = orderId;
                        trade["market_id"] = order["market_id"]; trade["side"] = isLong ? "long" : "short";
                        trade["notional_usdc"] = order["notional_usdc"];
                        trade["leverage_bps"] = order["leverage_bps"]; trade["max_slippage_bps"] = 0;
                        trade["owner_signature"] = ""; trade["is_limit_order"] = true;
                        try {
                            auto quote = createOpenQuote(trade);
                            quote["owner_signature"] = "";
                            quote["is_limit_order"] = true;
                            submitOpen(quote);
                        }
                        catch (const std::exception &error) { std::cerr << "limit order execution deferred: " << error.what() << '\n'; }
                    }
                }
            } catch (const std::exception &error) {
                std::cerr << "limit order worker cycle failed: " << error.what() << '\n';
            }
            for (unsigned i = 0; i < 100 && limitWorkerRunning_.load(); ++i)
                std::this_thread::sleep_for(std::chrono::milliseconds(100));
        }
    });
}

void Service::stopLimitWorker() {
    limitWorkerRunning_.store(false);
    if (limitWorker_.joinable()) limitWorker_.join();
}

} // namespace afterbell::trading
