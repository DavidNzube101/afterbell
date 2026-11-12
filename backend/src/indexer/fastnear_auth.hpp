#pragma once

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <string>

namespace afterbell::indexer {

inline std::string fastNearAuthorizationHeader(const std::string &url) {
    const auto scheme = url.find("://");
    if (scheme == std::string::npos) return {};

    const auto hostStart = scheme + 3;
    const auto hostEnd = url.find_first_of("/?#", hostStart);
    auto authority = url.substr(hostStart, hostEnd - hostStart);
    const auto userInfo = authority.rfind('@');
    if (userInfo != std::string::npos) authority.erase(0, userInfo + 1);
    const auto port = authority.find(':');
    auto host = authority.substr(0, port);
    std::transform(host.begin(), host.end(), host.begin(), [](unsigned char c) {
        return static_cast<char>(std::tolower(c));
    });

    constexpr char suffix[] = ".fastnear.com";
    if (host != "fastnear.com" &&
        (host.size() <= sizeof(suffix) - 1 ||
         host.compare(host.size() - (sizeof(suffix) - 1), sizeof(suffix) - 1, suffix) != 0)) {
        return {};
    }

    const auto *apiKey = std::getenv("FASTNEAR_API_KEY");
    if (!apiKey || !*apiKey) return {};
    return std::string("Authorization: Bearer ") + apiKey;
}

} // namespace afterbell::indexer
