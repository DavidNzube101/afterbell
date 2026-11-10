#include "oracle/worker.hpp"

#include "oracle/crypto.hpp"
#include "oracle/near_publisher.hpp"
#include "oracle/robinhood_pools.hpp"

#include <chrono>
#include <cstdlib>
#include <iostream>
#include <stdexcept>

namespace afterbell::oracle {
namespace {

std::uint64_t nowMs() {
    return static_cast<std::uint64_t>(std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::system_clock::now().time_since_epoch()).count());
}
void logEvent(const std::string &event,const std::string &market,const std::string &detail={}) {
    Json::Value value;value["event"]=event;if(!market.empty())value["market"]=market;if(!detail.empty())value["detail"]=detail;value["timestamp_ms"]=Json::UInt64(nowMs());
    Json::StreamWriterBuilder writer;writer["indentation"]="";std::cerr<<Json::writeString(writer,value)<<'\n';
}
}

Worker::Worker(WorkerConfig config):config_(std::move(config)) {
    if(config_.pollIntervalMs<5'000||config_.pollIntervalMs>60'000)throw std::invalid_argument("ORACLE_POLL_INTERVAL_MS must be between 5000 and 60000");
    if(config_.twapWindowSeconds<60||config_.twapWindowSeconds>3600)throw std::invalid_argument("ORACLE_TWAP_WINDOW_SECONDS must be between 60 and 3600");
    sampler_=std::make_unique<RobinhoodPoolSampler>(config_.robinhoodRpcUrl,config_.marketRegistryPath,config_.poolRegistryPath,config_.twapWindowSeconds);
    const auto signer=ethereumAddress(config_.evmSignerPrivateKey);
    publisher_=std::make_unique<NearPublisher>(config_.nearRpcUrl,config_.nearTransactionRpcUrl,config_.contractId,config_.relayerAccountId,config_.relayerPrivateKey,signer,config_.evmSignerPrivateKey,config_.network);
    status_["enabled"]=true;status_["status"]="starting";status_["sourceChain"]="robinhood";status_["sourceChainId"]=4663;status_["twapWindowSeconds"]=config_.twapWindowSeconds;
    status_["relayerAccountId"]=config_.relayerAccountId;status_["markets"]=Json::arrayValue;
    status_["lastSampleAtMs"]=Json::nullValue;status_["lastPublishedAtMs"]=Json::nullValue;
}
Worker::~Worker(){stop();}
void Worker::start(){if(thread_.joinable())return;stopping_=false;thread_=std::thread(&Worker::run,this);}
void Worker::stop(){stopping_=true;wake_.notify_all();if(thread_.joinable())thread_.join();}
Json::Value Worker::status() const {std::lock_guard<std::mutex> lock(statusMutex_);return status_;}

void Worker::run(){
    logEvent("oracle.worker_started","");
    bool signerChecked=false;
    while(!stopping_){
        try{
            auto samples=sampler_->sampleAll();
            const auto sampledAt=nowMs();
            if(!signerChecked){
                if(!publisher_->verifyContractOracleSigner()){
                    Json::Value markets(Json::arrayValue);
                    for(const auto &sample:samples){Json::Value m;m["symbol"]=sample.marketId;m["blockNumber"]=Json::UInt64(sample.blockNumber);m["price"]=Json::UInt64(sample.price);m["spotPrice"]=Json::UInt64(sample.spotPrice);m["spotTwapDeviationBps"]=Json::UInt64(sample.spotTwapDeviationBps);m["liquidity"]=Json::UInt64(sample.liquidity);m["status"]=sample.spotTwapDeviationBps<=50?"awaiting_signer":"quarantined";if(sample.spotTwapDeviationBps>50)m["reason"]="spot_twap_deviation_over_50bps";markets.append(std::move(m));}
                    {std::lock_guard<std::mutex> lock(statusMutex_);status_["status"]="signer_mismatch";status_["lastError"]="A1 oracle signer does not match ORACLE_SIGNER_PRIVATE_KEY";status_["lastSampleAtMs"]=Json::UInt64(sampledAt);status_["markets"]=markets;}
                    logEvent("oracle.signer_mismatch","");
                    std::unique_lock<std::mutex> lockWait(statusMutex_);wake_.wait_for(lockWait,std::chrono::milliseconds(config_.pollIntervalMs),[this]{return stopping_.load();});
                    continue;
                }
                signerChecked=true;
            }
            Json::Value markets(Json::arrayValue);
            bool allOk=true;
            for(const auto &sample:samples){
                Json::Value m;m["symbol"]=sample.marketId;m["blockNumber"]=Json::UInt64(sample.blockNumber);m["price"]=Json::UInt64(sample.price);m["spotPrice"]=Json::UInt64(sample.spotPrice);m["spotTwapDeviationBps"]=Json::UInt64(sample.spotTwapDeviationBps);m["liquidity"]=Json::UInt64(sample.liquidity);
                if(sample.spotTwapDeviationBps>50){m["status"]="quarantined";m["reason"]="spot_twap_deviation_over_50bps";allOk=false;logEvent("oracle.sample_quarantined",sample.marketId,"spot and TWAP exceed A1 hard bound");}
                else {
                    try{const auto hash=publisher_->publish(sample);if(hash.empty()){m["status"]="unchanged";}else{m["status"]="published";m["transactionHash"]=hash;{std::lock_guard<std::mutex> lock(statusMutex_);status_["lastPublishedAtMs"]=Json::UInt64(nowMs());}logEvent("oracle.attestation_published",sample.marketId,hash);}}
                    catch(const std::exception &e){m["status"]="publish_failed";m["reason"]=e.what();allOk=false;signerChecked=false;logEvent("oracle.publish_failed",sample.marketId,e.what());}
                }
                markets.append(std::move(m));
            }
            {std::lock_guard<std::mutex> lock(statusMutex_);status_["markets"]=markets;status_["lastSampleAtMs"]=Json::UInt64(sampledAt);status_["status"]=allOk?"running":"degraded";status_.removeMember("lastError");}
        }catch(const std::exception &e){
            std::lock_guard<std::mutex> lock(statusMutex_);status_["status"]="degraded";status_["lastError"]=e.what();logEvent("oracle.sample_failed","",e.what());
        }
        std::unique_lock<std::mutex> lock(statusMutex_);wake_.wait_for(lock,std::chrono::milliseconds(config_.pollIntervalMs),[this]{return stopping_.load();});
    }
    {std::lock_guard<std::mutex> lock(statusMutex_);status_["status"]="stopped";}
    logEvent("oracle.worker_stopped","");
}

} // namespace afterbell::oracle
