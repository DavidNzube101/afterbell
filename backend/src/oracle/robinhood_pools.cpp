#include "oracle/robinhood_pools.hpp"

#include "oracle/crypto.hpp"

#include <curl/curl.h>
#include <json/json.h>

#include <algorithm>
#include <cctype>
#include <cmath>
#include <fstream>
#include <iterator>
#include <memory>
#include <stdexcept>
#include <utility>

namespace afterbell::oracle {
namespace {

std::size_t appendResponse(char *data, std::size_t size, std::size_t count, void *context) {
    const auto n=size*count; static_cast<std::string *>(context)->append(data,n); return n;
}
std::string jsonText(const Json::Value &value) {
    Json::StreamWriterBuilder writer; writer["indentation"]=""; return Json::writeString(writer,value);
}
std::string rpcErrorText(const Json::Value &error) {
    std::string message;
    if (error["code"].isInt()) message="code "+std::to_string(error["code"].asInt());
    if (error["message"].isString()) {
        if (!message.empty()) message += ": ";
        message += error["message"].asString();
    }
    if (message.empty()) message="unspecified JSON-RPC error";
    constexpr std::size_t maxMessageLength=300;
    if (message.size()>maxMessageLength) message.resize(maxMessageLength);
    return message;
}
std::string lower(std::string value) {
    std::transform(value.begin(),value.end(),value.begin(),[](unsigned char c){return static_cast<char>(std::tolower(c));}); return value;
}
std::string padAddress(const std::string &address) {
    if(address.size()!=42||address.substr(0,2)!="0x")throw std::invalid_argument("invalid EVM address in oracle pool registry");
    return std::string(24,'0')+lower(address.substr(2));
}
std::uint64_t parseHexU64(const std::string &value, const char *field) {
    try { if(value.rfind("0x",0)!=0)throw std::runtime_error("bad hex"); return std::stoull(value.substr(2),nullptr,16); }
    catch(...) { throw std::runtime_error(std::string("Robinhood RPC returned invalid ")+field); }
}
std::string word(const std::string &data, std::size_t index) {
    const auto start=2+index*64;
    if(data.size()<start+64)throw std::runtime_error("Robinhood RPC returned short ABI data");
    return data.substr(start,64);
}
std::uint64_t wordU64(const std::string &data, std::size_t index) {
    const auto value=word(data,index);
    try { return std::stoull(value.substr(48),nullptr,16); }
    catch(...) { throw std::runtime_error("Robinhood RPC returned invalid ABI integer"); }
}
std::int64_t wordInt56(const std::string &data, std::size_t index) {
    const auto hex=word(data,index).substr(48);
    std::uint64_t raw=0;
    try { raw=std::stoull(hex,nullptr,16); }
    catch(...) { throw std::runtime_error("Robinhood RPC returned invalid tick cumulative"); }
    if(raw&(1ULL<<55))raw|=(~0ULL<<56);
    return static_cast<std::int64_t>(raw);
}
std::int32_t slotTick(const std::string &data) {
    const auto hex=word(data,1).substr(58); // int24 is right-aligned in the ABI word
    std::uint32_t raw=0;
    try { raw=static_cast<std::uint32_t>(std::stoul(hex,nullptr,16)); }
    catch(...) { throw std::runtime_error("Robinhood RPC returned invalid spot tick"); }
    if(raw&(1U<<23))raw|=0xff000000U;
    return static_cast<std::int32_t>(raw);
}
std::uint64_t tickPriceE8(std::int32_t tick, bool quoteIsToken1,
                          std::uint8_t assetDecimals, std::uint8_t quoteDecimals) {
    long double ratio=std::pow(1.0001L,static_cast<long double>(tick));
    ratio*=std::pow(10.0L,static_cast<int>(assetDecimals)-static_cast<int>(quoteDecimals));
    const auto price=quoteIsToken1?ratio:1.0L/ratio;
    const auto scaled=price*100'000'000.0L;
    if(!std::isfinite(scaled)||scaled<1.0L||scaled>static_cast<long double>(UINT64_MAX))throw std::runtime_error("Uniswap pool produced an invalid stock price");
    return static_cast<std::uint64_t>(std::llround(scaled));
}
std::uint64_t deviationBps(std::uint64_t price,std::uint64_t spot) {
    const auto diff=price>spot?price-spot:spot-price;
    return static_cast<std::uint64_t>((static_cast<unsigned __int128>(diff)*10'000)/price);
}

class EvmRpc {
  public:
    explicit EvmRpc(std::string endpoint):endpoint_(std::move(endpoint)) {
        static const bool initialized=[] { if(curl_global_init(CURL_GLOBAL_DEFAULT)!=CURLE_OK)throw std::runtime_error("could not initialize RPC HTTP client");return true;}();
        (void)initialized;
        if(endpoint_.empty())throw std::invalid_argument("ROBINHOOD_RPC_URL is required for oracle sampling");
    }
    Json::Value call(const std::string &method,const Json::Value &params) const {
        Json::Value payload; payload["jsonrpc"]="2.0";payload["id"]=1;payload["method"]=method;payload["params"]=params;
        const auto body=jsonText(payload);
        std::unique_ptr<CURL,decltype(&curl_easy_cleanup)> client(curl_easy_init(),curl_easy_cleanup);
        if(!client)throw std::runtime_error("could not allocate Robinhood RPC request");
        std::string response;
        struct curl_slist *headers=nullptr;headers=curl_slist_append(headers,"Content-Type: application/json");
        std::unique_ptr<curl_slist,decltype(&curl_slist_free_all)> guard(headers,curl_slist_free_all);
        curl_easy_setopt(client.get(),CURLOPT_URL,endpoint_.c_str());curl_easy_setopt(client.get(),CURLOPT_HTTPHEADER,guard.get());
        curl_easy_setopt(client.get(),CURLOPT_POST,1L);curl_easy_setopt(client.get(),CURLOPT_POSTFIELDS,body.data());curl_easy_setopt(client.get(),CURLOPT_POSTFIELDSIZE,static_cast<long>(body.size()));
        curl_easy_setopt(client.get(),CURLOPT_WRITEFUNCTION,appendResponse);curl_easy_setopt(client.get(),CURLOPT_WRITEDATA,&response);
        curl_easy_setopt(client.get(),CURLOPT_CONNECTTIMEOUT,8L);curl_easy_setopt(client.get(),CURLOPT_TIMEOUT,20L);curl_easy_setopt(client.get(),CURLOPT_NOSIGNAL,1L);curl_easy_setopt(client.get(),CURLOPT_FOLLOWLOCATION,0L);curl_easy_setopt(client.get(),CURLOPT_USERAGENT,"Afterbell-Oracle/0.1");
        const auto rc=curl_easy_perform(client.get());if(rc!=CURLE_OK)throw std::runtime_error(std::string("Robinhood RPC transport failed: ")+curl_easy_strerror(rc));
        long status=0;curl_easy_getinfo(client.get(),CURLINFO_RESPONSE_CODE,&status);if(status<200||status>=300)throw std::runtime_error("Robinhood RPC returned HTTP "+std::to_string(status));
        Json::CharReaderBuilder builder;Json::Value parsed;std::string errors;auto reader=std::unique_ptr<Json::CharReader>(builder.newCharReader());
        if(!reader->parse(response.data(),response.data()+response.size(),&parsed,&errors))
            throw std::runtime_error("Robinhood RPC returned invalid JSON for "+method);
        if(parsed.isMember("error"))
            throw std::runtime_error("Robinhood RPC "+method+" failed: "+rpcErrorText(parsed["error"]));
        if(!parsed.isMember("result"))
            throw std::runtime_error("Robinhood RPC response omitted result for "+method);
        return parsed["result"];
    }
    std::string ethCall(const std::string &to,const std::string &data) const {
        Json::Value args(Json::arrayValue);Json::Value call;call["to"]=to;call["data"]=data;args.append(call);args.append("latest");
        Json::Value result;
        try {
            result=this->call("eth_call",args);
        } catch(const std::exception &error) {
            const auto selector=data.size()>=10?data.substr(0,10):std::string("unknown");
            throw std::runtime_error("Robinhood eth_call to "+to+" (selector "+selector+") failed: "+error.what());
        }
        if(!result.isString())throw std::runtime_error("Robinhood RPC eth_call returned invalid data");return result.asString();
    }
  private: std::string endpoint_;
};

} // namespace

struct RobinhoodPoolSampler::Impl {
    struct Market { std::string symbol,token,pool;std::uint32_t fee; };
    std::string rpcUrl,quoteToken,factory;
    std::uint8_t quoteDecimals;
    std::uint32_t windowSeconds;
    std::vector<Market> markets;
    Impl(std::string endpoint,std::string marketPath,std::string poolPath,std::uint32_t window):rpcUrl(std::move(endpoint)),windowSeconds(window) {
        Json::Value marketRoot,poolRoot;Json::CharReaderBuilder builder;std::string errors;
        auto parse=[&](const std::string &path,Json::Value &into){std::ifstream file(path);if(!file)throw std::runtime_error("could not open oracle market configuration");std::string contents((std::istreambuf_iterator<char>(file)),{});auto reader=std::unique_ptr<Json::CharReader>(builder.newCharReader());if(!reader->parse(contents.data(),contents.data()+contents.size(),&into,&errors))throw std::runtime_error("oracle market configuration JSON is invalid");};
        parse(marketPath,marketRoot);parse(poolPath,poolRoot);
        if(!poolRoot["chainId"].isUInt()||poolRoot["chainId"].asUInt()!=4663)throw std::runtime_error("oracle pool registry is not for Robinhood Chain");
        quoteToken=poolRoot["quoteToken"].asString();factory=poolRoot["factory"].asString();quoteDecimals=static_cast<std::uint8_t>(poolRoot["quoteDecimals"].asUInt());
        if(!poolRoot["markets"].isObject()||!marketRoot["Robinhood"].isObject()||quoteDecimals>18||windowSeconds<60||windowSeconds>3600)throw std::runtime_error("oracle pool registry is incomplete or invalid");
        for(const auto &symbol:poolRoot["markets"].getMemberNames()){
            if(!marketRoot["Robinhood"][symbol].isString())throw std::runtime_error("oracle pool has no matching stock token in market registry");
            const auto &m=poolRoot["markets"][symbol];
            const auto fee=m["fee"].asUInt();if(fee!=100&&fee!=500&&fee!=3000&&fee!=10000)throw std::runtime_error("oracle pool fee tier is invalid");
            markets.push_back({symbol,marketRoot["Robinhood"][symbol].asString(),m["pool"].asString(),fee});
        }
        std::sort(markets.begin(),markets.end(),[](const auto &a,const auto &b){return a.symbol<b.symbol;});
    }
};

RobinhoodPoolSampler::RobinhoodPoolSampler(std::string rpcUrl,std::string marketRegistryPath,std::string poolRegistryPath,std::uint32_t twapWindowSeconds)
    :impl_(std::make_unique<Impl>(std::move(rpcUrl),std::move(marketRegistryPath),std::move(poolRegistryPath),twapWindowSeconds)){}
RobinhoodPoolSampler::~RobinhoodPoolSampler()=default;

std::vector<PoolPriceSample> RobinhoodPoolSampler::sampleAll() const {
    EvmRpc rpc(impl_->rpcUrl);
    const auto chainId=rpc.call("eth_chainId",Json::Value(Json::arrayValue));
    if(!chainId.isString()||parseHexU64(chainId.asString(),"chain id")!=4663)throw std::runtime_error("configured Robinhood RPC is not chain 4663");
    Json::Value blockParams(Json::arrayValue);blockParams.append("latest");blockParams.append(false);
    const auto head=rpc.call("eth_getBlockByNumber",blockParams);
    const auto blockNumber=parseHexU64(head["number"].asString(),"block number");
    const auto blockTime=parseHexU64(head["timestamp"].asString(),"block timestamp");
    const auto blockHash=head["hash"].asString();
    if(blockNumber==0||blockTime==0||blockHash.size()!=66)throw std::runtime_error("Robinhood RPC returned invalid latest block");
    const std::string round="rhc:"+std::to_string(blockNumber)+":"+blockHash.substr(2);
    if(wordU64(rpc.ethCall(impl_->quoteToken,"0x313ce567"),0)!=impl_->quoteDecimals)
        throw std::runtime_error("configured USDG decimals do not match the Robinhood token");
    std::vector<PoolPriceSample> samples;
    for(const auto &market:impl_->markets){
        auto tokenA=lower(market.token),tokenB=lower(impl_->quoteToken);if(tokenB<tokenA)std::swap(tokenA,tokenB);
        char feeHex[7];std::snprintf(feeHex,sizeof(feeHex),"%06x",market.fee);
        const auto factoryData="0x1698ee82"+padAddress(tokenA)+padAddress(tokenB)+std::string(58,'0')+feeHex;
        const auto factoryPool="0x"+word(rpc.ethCall(impl_->factory,factoryData),0).substr(24);
        if(lower(factoryPool)!=lower(market.pool))throw std::runtime_error("configured pool is not the registered Uniswap V3 pool for "+market.symbol);
        const auto token0Data=rpc.ethCall(market.pool,"0x0dfe1681");
        const auto token1Data=rpc.ethCall(market.pool,"0xd21220a7");
        const auto token0="0x"+word(token0Data,0).substr(24),token1="0x"+word(token1Data,0).substr(24);
        const auto quote=lower(impl_->quoteToken),token=lower(market.token);
        const bool quoteIsToken0=lower(token0)==quote,quoteIsToken1=lower(token1)==quote;
        if((!quoteIsToken0&&!quoteIsToken1)||lower(quoteIsToken0?token1:token0)!=token)
            throw std::runtime_error("configured Uniswap V3 pool token pair does not match "+market.symbol+"/USDG");
        const auto assetToken=quoteIsToken0?token1:token0;
        const auto decimalsData=rpc.ethCall(assetToken,"0x313ce567");
        const auto assetDecimals=wordU64(decimalsData,0);
        if(assetDecimals>24)throw std::runtime_error("Robinhood stock token has unsupported decimals");
        const auto liquidityData=rpc.ethCall(market.pool,"0x1a686502");
        const auto liquidity=wordU64(liquidityData,0);
        if(liquidity==0)throw std::runtime_error("Uniswap V3 pool has no active liquidity for "+market.symbol);
        const auto slot=rpc.ethCall(market.pool,"0x3850c7bd");
        const auto spot=slotTick(slot);
        const auto observeData="0x883bdbfd"+std::string(63,'0')+"20"+std::string(63,'0')+"2"+std::string(61,'0')+"12c"+std::string(64,'0');
        const auto observed=rpc.ethCall(market.pool,observeData);
        const auto firstOffset=wordU64(observed,0),secondOffset=wordU64(observed,1);
        if(firstOffset%32||secondOffset%32||firstOffset/32+2>=observed.size()/64||secondOffset/32>=observed.size()/64)throw std::runtime_error("Uniswap V3 observe returned malformed data");
        const auto firstIndex=static_cast<std::size_t>(firstOffset/32+1),secondIndex=firstIndex+1;
        const auto tickBefore=wordInt56(observed,firstIndex),tickAfter=wordInt56(observed,secondIndex);
        const auto meanDelta=tickAfter-tickBefore;
        auto mean=meanDelta/static_cast<std::int64_t>(impl_->windowSeconds);
        if(meanDelta<0&&meanDelta%static_cast<std::int64_t>(impl_->windowSeconds)!=0)--mean;
        if(mean < -887272 || mean > 887272)throw std::runtime_error("Uniswap V3 TWAP tick is outside protocol bounds");
        const auto price=tickPriceE8(static_cast<std::int32_t>(mean),quoteIsToken1,static_cast<std::uint8_t>(assetDecimals),impl_->quoteDecimals);
        const auto spotPrice=tickPriceE8(spot,quoteIsToken1,static_cast<std::uint8_t>(assetDecimals),impl_->quoteDecimals);
        const auto deviation=deviationBps(price,spotPrice);
        samples.push_back({PriceAttestation{market.symbol,price,spotPrice,25,blockTime*1000,round},blockNumber,deviation,liquidity});
    }
    return samples;
}

} // namespace afterbell::oracle
