#include "http/request_logger.hpp"

#include <drogon/HttpAppFramework.h>

#include <atomic>
#include <chrono>
#include <cstdint>
#include <iostream>
#include <mutex>
#include <string>
#include <unordered_map>
#include <utility>

namespace afterbell {
namespace {

struct RequestContext {
    std::string id;
    std::chrono::steady_clock::time_point startedAt;
};

std::mutex contextsMutex;
std::mutex outputMutex;
std::unordered_map<const drogon::HttpRequest *, RequestContext> contexts;
std::atomic<std::uint64_t> requestSequence{0};

std::string makeRequestId() {
    const auto now = std::chrono::duration_cast<std::chrono::milliseconds>(
                         std::chrono::system_clock::now().time_since_epoch())
                         .count();
    return std::to_string(now) + "-" +
           std::to_string(requestSequence.fetch_add(1, std::memory_order_relaxed) + 1);
}

} // namespace

void registerRequestLogger() {
    auto &app = drogon::app();
    app.registerPreRoutingAdvice([](const drogon::HttpRequestPtr &request) {
        RequestContext context{makeRequestId(), std::chrono::steady_clock::now()};
        std::lock_guard<std::mutex> lock(contextsMutex);
        contexts[request.get()] = std::move(context);
    });

    app.registerPreSendingAdvice([](const drogon::HttpRequestPtr &request,
                                    const drogon::HttpResponsePtr &response) {
        RequestContext context;
        {
            std::lock_guard<std::mutex> lock(contextsMutex);
            const auto found = contexts.find(request.get());
            if (found == contexts.end()) return;
            context = std::move(found->second);
            contexts.erase(found);
        }

        const auto now = std::chrono::steady_clock::now();
        const auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(
                                  now - context.startedAt)
                                  .count();
        const auto timestamp = std::chrono::duration_cast<std::chrono::milliseconds>(
                                   std::chrono::system_clock::now().time_since_epoch())
                                   .count();
        response->addHeader("X-Request-ID", context.id);

        Json::Value entry;
        entry["event"] = "http.request";
        entry["timestamp_ms"] = static_cast<Json::Int64>(timestamp);
        entry["request_id"] = context.id;
        entry["method"] = request->getMethodString();
        entry["path"] = request->path(); // Exclude query parameters from logs.
        entry["status"] = static_cast<int>(response->getStatusCode());
        entry["duration_ms"] = static_cast<Json::Int64>(duration);
        Json::StreamWriterBuilder writer;
        writer["indentation"] = "";
        std::lock_guard<std::mutex> outputLock(outputMutex);
        std::cout << Json::writeString(writer, entry) << '\n' << std::flush;
    });
}

} // namespace afterbell
