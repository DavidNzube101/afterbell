#pragma once

#include "indexer/event.hpp"

#include <libpq-fe.h>

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace afterbell::indexer {

class Database {
  public:
    explicit Database(const std::string &connectionString);
    ~Database();

    Database(const Database &) = delete;
    Database &operator=(const Database &) = delete;

    std::uint64_t ensureCursor(const std::string &network,
                               std::optional<std::uint64_t> startBlock);
    void saveFinalizedBlock(const std::string &network,
                            std::uint64_t height,
                            std::uint64_t expectedCursor,
                            const std::vector<DepositEvent> &events);

  private:
    PGconn *connection_{};
    void execute(const std::string &sql);
    PGresult *query(const std::string &sql,
                    const std::vector<std::string> &parameters = {});
};

} // namespace afterbell::indexer
