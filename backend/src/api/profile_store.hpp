#pragma once

#include <json/json.h>
#include <stdexcept>
#include <string>

namespace afterbell::api {

class UsernameUnavailable : public std::runtime_error {
  public:
    using std::runtime_error::runtime_error;
};

class ProfileStore {
  public:
    explicit ProfileStore(std::string databaseUrl);
    Json::Value claimUsername(const std::string &accountId, const std::string &username) const;
    Json::Value reclaimUsername(const std::string &accountId, const std::string &email) const;

  private:
    std::string databaseUrl_;
    Json::Value persist(const std::string &accountId, const std::string &username,
                        bool fromWaitlist) const;
};

} // namespace afterbell::api
