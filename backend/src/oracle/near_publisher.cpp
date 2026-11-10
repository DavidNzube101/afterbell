#include "oracle/near_publisher.hpp"

#include "indexer/near_rpc.hpp"
#include "oracle/crypto.hpp"
#include "oracle/near_tx_lock.hpp"

#include <openssl/evp.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <memory>
#include <stdexcept>
#include <utility>

namespace afterbell::oracle {
namespace {

std::string compactJson(const Json::Value &value) {
    Json::StreamWriterBuilder builder;
    builder["indentation"] = "";
    return Json::writeString(builder, value);
}

void appendU32(std::vector<std::uint8_t> &out, std::uint32_t value) {
    for (unsigned i=0;i<4;++i) out.push_back(static_cast<std::uint8_t>(value >> (8*i)));
}
void appendU64(std::vector<std::uint8_t> &out, std::uint64_t value) {
    for (unsigned i=0;i<8;++i) out.push_back(static_cast<std::uint8_t>(value >> (8*i)));
}
void appendString(std::vector<std::uint8_t> &out, const std::string &value) {
    if (value.size() > UINT32_MAX) throw std::length_error("NEAR transaction string too large");
    appendU32(out, static_cast<std::uint32_t>(value.size()));
    out.insert(out.end(), value.begin(), value.end());
}
void appendBytes(std::vector<std::uint8_t> &out, const std::vector<std::uint8_t> &value) {
    if (value.size() > UINT32_MAX) throw std::length_error("NEAR transaction argument too large");
    appendU32(out, static_cast<std::uint32_t>(value.size()));
    out.insert(out.end(), value.begin(), value.end());
}
std::vector<std::uint8_t> parseEd25519Key(const std::string &key) {
    constexpr const char *prefix = "ed25519:";
    if (key.rfind(prefix, 0) != 0) throw std::invalid_argument("NEAR publisher key must use ed25519 format");
    auto decoded = base58Decode(key.substr(8));
    if (decoded.size() != 64) throw std::invalid_argument("NEAR publisher key must contain a 64-byte keypair");
    return decoded;
}
std::array<std::uint8_t, 32> ed25519Public(const std::vector<std::uint8_t> &secret) {
    using Pkey = std::unique_ptr<EVP_PKEY, decltype(&EVP_PKEY_free)>;
    Pkey key(EVP_PKEY_new_raw_private_key(EVP_PKEY_ED25519, nullptr, secret.data(), 32), EVP_PKEY_free);
    std::array<std::uint8_t,32> pub{};
    std::size_t length = pub.size();
    if (!key || EVP_PKEY_get_raw_public_key(key.get(), pub.data(), &length) != 1 || length != pub.size())
        throw std::runtime_error("could not derive NEAR relayer public key");
    return pub;
}
std::vector<std::uint8_t> ed25519Sign(const std::vector<std::uint8_t> &secret,
                                     const std::uint8_t *data, std::size_t size) {
    using Pkey = std::unique_ptr<EVP_PKEY, decltype(&EVP_PKEY_free)>;
    using Ctx = std::unique_ptr<EVP_MD_CTX, decltype(&EVP_MD_CTX_free)>;
    Pkey key(EVP_PKEY_new_raw_private_key(EVP_PKEY_ED25519, nullptr, secret.data(), 32), EVP_PKEY_free);
    Ctx ctx(EVP_MD_CTX_new(), EVP_MD_CTX_free);
    std::vector<std::uint8_t> sig(64);
    std::size_t length = sig.size();
    if (!key || !ctx || EVP_DigestSignInit(ctx.get(), nullptr, nullptr, nullptr, key.get()) != 1 ||
        EVP_DigestSign(ctx.get(), sig.data(), &length, data, size) != 1 || length != sig.size())
        throw std::runtime_error("NEAR transaction signing failed");
    return sig;
}

std::uint64_t asU64(const Json::Value &value, const char *field) {
    try {
        if (value.isString()) return std::stoull(value.asString());
        if (value.isUInt64()) return value.asUInt64();
        if (value.isInt64() && value.asInt64() >= 0) return static_cast<std::uint64_t>(value.asInt64());
    } catch (...) {}
    throw std::runtime_error(std::string("NEAR response omitted valid ") + field);
}

Json::Value viewCall(afterbell::indexer::NearRpc &rpc, const std::string &account,
                     const std::string &method, const Json::Value &args) {
    Json::Value request;
    request["request_type"] = "call_function";
    request["finality"] = "final";
    request["account_id"] = account;
    request["method_name"] = method;
    const auto serialized = compactJson(args);
    request["args_base64"] = base64Encode(reinterpret_cast<const std::uint8_t *>(serialized.data()), serialized.size());
    const auto result = rpc.call("query", request);
    if (!result["result"].isArray()) throw std::runtime_error("NEAR view call returned no result bytes");
    std::string bytes;
    bytes.reserve(result["result"].size());
    for (const auto &byte : result["result"]) {
        if (!byte.isUInt() || byte.asUInt() > 255) throw std::runtime_error("NEAR view call returned invalid result bytes");
        bytes.push_back(static_cast<char>(byte.asUInt()));
    }
    Json::CharReaderBuilder builder;
    Json::Value output;
    std::string errors;
    auto reader=std::unique_ptr<Json::CharReader>(builder.newCharReader());
    if (!reader->parse(bytes.data(),bytes.data()+bytes.size(),&output,&errors)) throw std::runtime_error("NEAR view call returned invalid JSON");
    return output;
}

} // namespace

NearPublisher::NearPublisher(std::string rpcUrl, std::string transactionRpcUrl,
                             std::string contractId,
                             std::string accountId, std::string privateKey,
                             std::string oracleSignerAddress,
                             std::string oracleSignerPrivateKey,
                             std::string expectedNetwork)
    : rpcUrl_(std::move(rpcUrl)), transactionRpcUrl_(std::move(transactionRpcUrl)),
      contractId_(std::move(contractId)),
      accountId_(std::move(accountId)), privateKey_(std::move(privateKey)),
      oracleSignerAddress_(std::move(oracleSignerAddress)),
      oracleSignerPrivateKey_(std::move(oracleSignerPrivateKey)),
      expectedNetwork_(std::move(expectedNetwork)) {
    if (rpcUrl_.empty() || transactionRpcUrl_.empty() || contractId_.empty() || accountId_.empty()) throw std::invalid_argument("oracle NEAR RPC, transaction RPC, contract, and relayer account are required");
    const auto key=parseEd25519Key(privateKey_);
    const auto pub=ed25519Public(key);
    if (!std::equal(pub.begin(),pub.end(),key.begin()+32)) throw std::invalid_argument("NEAR relayer keypair public key does not match its seed");
}
NearPublisher::~NearPublisher() = default;

bool NearPublisher::verifyContractOracleSigner() const {
    afterbell::indexer::NearRpc rpc(rpcUrl_);
    if (expectedNetwork_ != "testnet" && expectedNetwork_ != "mainnet") throw std::invalid_argument("NEAR_NETWORK must be testnet or mainnet");
    if (rpc.chainId() != expectedNetwork_) throw std::runtime_error("oracle NEAR RPC chain does not match NEAR_NETWORK");
    auto value=viewCall(rpc,contractId_,"oracle_signer",Json::Value(Json::objectValue));
    if (!value.isArray() || value.size()!=20 || oracleSignerAddress_.size()!=42 || oracleSignerAddress_.substr(0,2)!="0x") return false;
    for(std::size_t i=0;i<20;++i) {
        unsigned expected=0;
        try { expected=static_cast<unsigned>(std::stoul(oracleSignerAddress_.substr(2+2*i,2),nullptr,16)); } catch (...) { return false; }
        if (!value[static_cast<Json::ArrayIndex>(i)].isUInt() || value[static_cast<Json::ArrayIndex>(i)].asUInt()!=expected) return false;
    }
    return true;
}

std::string NearPublisher::publish(const PriceAttestation &price) {
    std::lock_guard<std::mutex> lock(nearTransactionMutex());
    afterbell::indexer::NearRpc rpc(rpcUrl_);
    Json::Value queryArgs; queryArgs["market_id"] = price.marketId;
    const auto old=viewCall(rpc,contractId_,"market_price",queryArgs);
    const std::uint64_t sequence=old.isObject() ? asU64(old["sequence"],"market price sequence")+1 : 1;
    Json::Value headArgs; headArgs["finality"]="final";
    const auto head=rpc.call("block",headArgs);
    const auto chainNowMs=asU64(head["header"]["timestamp_nanosec"],"NEAR block timestamp")/1'000'000;
    if (price.observedAtMs>chainNowMs || chainNowMs-price.observedAtMs>60'000) throw std::runtime_error("oracle sample is stale or future dated relative to NEAR block time");
    if (price.price==0 || price.spotPrice==0 || price.sourceRound.empty() || price.sourceRound.size()>128) throw std::invalid_argument("oracle attestation has invalid price or source round");
    const auto deviation=(price.price>price.spotPrice?price.price-price.spotPrice:price.spotPrice-price.price)*10'000/price.price;
    if (deviation>50) throw std::runtime_error("oracle sample spot and TWAP exceed A1's 50 bps bound");
    if (old.isObject()) {
        const auto oldPrice=asU64(old["price"],"market price");
        const auto oldObserved=asU64(old["observed_at_ms"],"market price timestamp");
        const auto diff=oldPrice>price.price?oldPrice-price.price:price.price-oldPrice;
        const auto move=oldPrice==0?UINT64_MAX:static_cast<std::uint64_t>((static_cast<unsigned __int128>(diff)*10'000)/oldPrice);
        if(chainNowMs>=oldObserved&&chainNowMs-oldObserved<45'000&&move<10)return {};
    }
    const auto expiry=chainNowMs+45'000;
    std::string message="afterbell:v1\ncontract:"+contractId_+"\naction:publish_market_price\nmarket:"+price.marketId+"\nprice:"+std::to_string(price.price)+"\nspot_price:"+std::to_string(price.spotPrice)+"\nconfidence_bps:"+std::to_string(price.confidenceBps)+"\nobserved_at_ms:"+std::to_string(price.observedAtMs)+"\nsequence:"+std::to_string(sequence)+"\nsource_round:"+price.sourceRound+"\nexpires_at_ms:"+std::to_string(expiry);
    const auto evmSignature=eip191Sign(message, oracleSignerPrivateKey_);

    Json::Value attestation;
    attestation["market_id"]=price.marketId;
    attestation["price"]=std::to_string(price.price);
    attestation["spot_price"]=std::to_string(price.spotPrice);
    attestation["confidence_bps"]=price.confidenceBps;
    attestation["observed_at_ms"]=Json::UInt64(price.observedAtMs);
    attestation["sequence"]=Json::UInt64(sequence);
    attestation["source_round"]=price.sourceRound;
    attestation["expires_at_ms"]=Json::UInt64(expiry);
    Json::Value args; args["attestation"]=attestation; args["signature"]=base64Encode(evmSignature.data(),evmSignature.size());
    const auto argsText=compactJson(args);

    const auto key=parseEd25519Key(privateKey_);
    const auto pub=ed25519Public(key);
    const auto publicKeyText="ed25519:"+base58Encode(pub.data(),pub.size());
    // Read the nonce and recent block from the same provider that will accept
    // the broadcast. A separate read endpoint can lag and return an already
    // consumed relayer nonce.
    afterbell::indexer::NearRpc transactionRpc(transactionRpcUrl_);
    Json::Value accessArgs;
    accessArgs["request_type"]="view_access_key"; accessArgs["finality"]="final";
    accessArgs["account_id"]=accountId_; accessArgs["public_key"]=publicKeyText;
    const auto access=transactionRpc.call("query",accessArgs);
    const auto nonce=asU64(access["nonce"],"relayer nonce")+1;
    Json::Value blockArgs; blockArgs["finality"]="final";
    const auto block=transactionRpc.call("block",blockArgs);
    const auto blockHash=base58Decode(block["header"]["hash"].asString());
    if (blockHash.size()!=32) throw std::runtime_error("NEAR returned an invalid block hash");

    std::vector<std::uint8_t> tx;
    appendString(tx,accountId_);
    tx.push_back(0); tx.insert(tx.end(),pub.begin(),pub.end());
    appendU64(tx,nonce);
    appendString(tx,contractId_);
    tx.insert(tx.end(),blockHash.begin(),blockHash.end());
    appendU32(tx,1); tx.push_back(2); // one FunctionCall action
    appendString(tx,"publish_market_price_attestation");
    appendBytes(tx,std::vector<std::uint8_t>(argsText.begin(),argsText.end()));
    appendU64(tx,150'000'000'000'000ULL);
    tx.insert(tx.end(),16,0); // zero yoctoNEAR deposit
    const auto txHash=sha256(tx.data(),tx.size());
    const auto nearSignature=ed25519Sign(key,txHash.data(),txHash.size());
    std::vector<std::uint8_t> signedTx=tx;
    signedTx.push_back(0); signedTx.insert(signedTx.end(),nearSignature.begin(),nearSignature.end());
    Json::Value params(Json::arrayValue); params.append(base64Encode(signedTx.data(),signedTx.size()));
    const auto receipt=transactionRpc.call("broadcast_tx_commit",params);
    if (receipt["status"].isMember("Failure") || receipt["final_execution_status"]=="FAILURE") throw std::runtime_error("A1 rejected oracle publication");
    const auto &hash=receipt["transaction"]["hash"];
    return hash.isString()?hash.asString():std::string("submitted");
}

} // namespace afterbell::oracle
