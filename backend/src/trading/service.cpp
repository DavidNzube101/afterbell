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
#include <limits>
#include <memory>
#include <stdexcept>
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

    Json::Value args; args["market_id"] = marketId;
    const auto market = viewCall(rpc, config.contractId, "perp_market", args);
    if (market.isNull() || !market["enabled"].asBool() || market["close_only"].asBool()) throw std::runtime_error("market is not accepting new positions");
    if (leverage > u64(market["max_leverage_bps"], "market max leverage") ||
        notional > u128(market["max_position_usdc"], "market position cap") ||
        slippage > u64(market["max_execution_deviation_bps"], "market slippage cap")) throw std::invalid_argument("order exceeds the market risk limits");
    if (viewCall(rpc, config.contractId, "perp_paused", Json::Value(Json::objectValue)).asBool()) throw std::runtime_error("perpetual trading is paused");
    args.clear(); args["account_id"] = account;
    if (viewCall(rpc, config.contractId, "owner_of", args).isNull()) throw std::invalid_argument("account has no registered owner");
    args.clear(); args["market_id"] = marketId;
    const auto price = viewCall(rpc, config.contractId, "market_price", args);
    if (price.isNull()) throw std::runtime_error("market oracle price is unavailable");
    const auto chainNow = blockTimeMs(rpc);
    const auto observedAt = u64(price["observed_at_ms"], "oracle timestamp");
    if (observedAt > chainNow || chainNow - observedAt > 60'000) throw std::runtime_error("market oracle price is stale");
    const auto health = viewCall(rpc, config.contractId, "oracle_health", args);
    if (health["circuit_broken"].asBool() ||
        u64(health["confidence_bps"], "oracle confidence") > u64(market["max_oracle_confidence_bps"], "market confidence cap") ||
        u64(health["spot_twap_deviation_bps"], "oracle deviation") > u64(market["max_execution_deviation_bps"], "market deviation cap")) throw std::runtime_error("oracle health checks do not allow opening this market");
    const auto sequence = u64(price["sequence"], "oracle sequence");
    const auto referencePrice = u128(price["price"], "oracle price");
    const auto nonce = tradingNonce(rpc, config.contractId, account) + 1;
    const auto expiry = chainNow + 30'000;
    const auto margin = (notional * 10'000 + leverage - 1) / leverage;
    const auto fee = (notional * config.feeBps + 9'999) / 10'000;
    args.clear(); args["account_id"] = account;
    const auto balance = u128(viewCall(rpc, config.contractId, "balance_of", args), "USDC balance");
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
    const auto messageValue = viewCall(rpc, config.contractId, "trade_quote_message", quoteArgs);
    if (!messageValue.isString()) throw std::runtime_error("A1 did not return a trade authorization message");

    Json::Value signer = viewCall(rpc, config.contractId, "perp_quote_signer", Json::Value(Json::objectValue));
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
    return quote;
}

} // namespace

Service::Service(Config config) : config_(std::move(config)) {
    if (config_.network != "testnet" && config_.network != "mainnet") throw std::invalid_argument("NEAR_NETWORK must be testnet or mainnet");
    if (config_.contractId.empty() || config_.readRpcUrl.empty() || config_.transactionRpcUrl.empty() || config_.relayerAccountId.empty()) throw std::invalid_argument("trading RPC, contract, and relayer configuration are required");
    (void)nearPublic(nearKey(config_.relayerPrivateKey));
    (void)afterbell::oracle::ethereumAddress(config_.quoteSignerPrivateKey);
    if (config_.feeBps > 100) throw std::invalid_argument("PERP_TRADE_FEE_BPS must be between 0 and 100");
}

Json::Value Service::createOpenQuote(const Json::Value &request) const {
    afterbell::indexer::NearRpc rpc(config_.readRpcUrl);
    if (rpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    return checkedQuote(rpc, config_, request, true);
}

Json::Value Service::submitOpen(const Json::Value &request) {
    std::lock_guard<std::mutex> lock(afterbell::oracle::nearTransactionMutex());
    if (!request["owner_signature"].isString()) throw std::invalid_argument("owner_signature is required");
    const auto ownerSignature = afterbell::oracle::base64Decode(request["owner_signature"].asString());
    if (ownerSignature.size() != 64 && ownerSignature.size() != 65) throw std::invalid_argument("owner signature has an invalid length");
    auto quote = request;
    quote.removeMember("owner_signature"); quote.removeMember("quote_signature"); quote.removeMember("message");
    afterbell::indexer::NearRpc readRpc(config_.readRpcUrl);
    if (readRpc.chainId() != config_.network) throw std::runtime_error("trade read RPC is on the wrong NEAR network");
    const auto verified = checkedQuote(readRpc, config_, quote, false);
    if (verified["message"].asString() != request["message"].asString()) throw std::runtime_error("trade message does not match quote fields");

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

} // namespace afterbell::trading
