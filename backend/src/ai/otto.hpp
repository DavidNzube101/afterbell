#pragma once

#include <drogon/HttpResponse.h>
#include <json/json.h>

#include <string>
#include <functional>

namespace afterbell::ai {

struct ProviderConfig {
    std::string nearApiKey;
    std::string nearModel;
    std::string groqApiKey;
    std::string groqModel;
};

Json::Value providerStatus(const ProviderConfig &config);
bool validChatRequest(const Json::Value &input, std::string &error);
bool allowRequestForAccount(const std::string &accountId);
Json::Value createWalletChallenge(const std::string &accountId, const Json::Value &owner, const std::string &recipient);
std::string walletChallengeAccount(const std::string &challengeId);
std::string verifyWalletChallenge(const std::string &challengeId, const std::string &signatureBase64, const Json::Value &currentOwner);
std::string walletSessionAccount(const std::string &token);
drogon::HttpResponsePtr streamChat(Json::Value input, ProviderConfig config,
                                  std::function<Json::Value()> contextProvider = {});

} // namespace afterbell::ai
