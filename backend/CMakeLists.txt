cmake_minimum_required(VERSION 3.22)
project(afterbell_backend VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(PostgreSQL REQUIRED)
find_package(OpenSSL REQUIRED COMPONENTS Crypto)
find_package(PkgConfig REQUIRED)
pkg_check_modules(CURL REQUIRED IMPORTED_TARGET libcurl)
pkg_check_modules(JSONCPP REQUIRED IMPORTED_TARGET jsoncpp)
option(AFTERBELL_REQUIRE_DROGON "Fail configure if Drogon is unavailable" OFF)
if(AFTERBELL_REQUIRE_DROGON)
    find_package(Drogon REQUIRED)
else()
    find_package(Drogon QUIET)
endif()

add_library(afterbell-indexer-core
    src/api/dashboard_store.cpp
    src/indexer/event.cpp
    src/indexer/fastnear_client.cpp
    src/indexer/near_rpc.cpp
    src/indexer/database.cpp
    src/indexer/indexer.cpp
    src/indexer/worker.cpp
    src/oracle/crypto.cpp
    src/oracle/near_publisher.cpp
    src/oracle/robinhood_pools.cpp
    src/oracle/worker.cpp
    src/trading/service.cpp
)
target_include_directories(afterbell-indexer-core PUBLIC src)
target_link_libraries(afterbell-indexer-core
    PUBLIC PostgreSQL::PostgreSQL PkgConfig::CURL PkgConfig::JSONCPP OpenSSL::Crypto
)
target_compile_options(afterbell-indexer-core PRIVATE -Wall -Wextra -Wpedantic)

add_executable(afterbell-indexer src/indexer/main.cpp)
target_link_libraries(afterbell-indexer PRIVATE afterbell-indexer-core)
target_compile_options(afterbell-indexer PRIVATE -Wall -Wextra -Wpedantic)

if(Drogon_FOUND)
    add_executable(afterbell-api
        src/api/main.cpp
        src/ai/otto.cpp
        src/api/profile_store.cpp
        src/http/request_logger.cpp
    )
    target_link_libraries(afterbell-api PRIVATE afterbell-indexer-core Drogon::Drogon)
    target_compile_options(afterbell-api PRIVATE -Wall -Wextra -Wpedantic)
else()
    message(STATUS "Drogon not found; skipping afterbell-api (install Drogon to build the HTTP API)")
endif()

include(CTest)
if(BUILD_TESTING)
    add_executable(afterbell-indexer-tests tests/indexer_tests.cpp)
    target_link_libraries(afterbell-indexer-tests PRIVATE afterbell-indexer-core)
    target_compile_options(afterbell-indexer-tests PRIVATE -Wall -Wextra -Wpedantic)
    add_test(NAME afterbell-indexer-tests COMMAND afterbell-indexer-tests)
endif()
