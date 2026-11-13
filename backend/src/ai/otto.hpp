#pragma once

#include <drogon/HttpResponse.h>
#include <json/json.h>

#include <string>

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
drogon::HttpResponsePtr streamChat(Json::Value input, ProviderConfig config);

} // namespace afterbell::ai
