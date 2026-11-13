#pragma once

#include <array>
#include <cstdint>
#include <string>
#include <vector>

namespace afterbell::oracle {

std::array<std::uint8_t, 32> keccak256(const std::uint8_t *data, std::size_t size);
std::array<std::uint8_t, 32> sha256(const std::uint8_t *data, std::size_t size);
std::vector<std::uint8_t> eip191Sign(const std::string &message,
                                     const std::string &privateKeyHex);
bool eip191Verify(const std::string &message, const std::vector<std::uint8_t> &signature,
                  const std::vector<std::uint8_t> &expectedAddress);
std::string ethereumAddress(const std::string &privateKeyHex);
std::vector<std::uint8_t> base58Decode(const std::string &input);
std::string base58Encode(const std::uint8_t *data, std::size_t size);
std::string base64Encode(const std::uint8_t *data, std::size_t size);
std::vector<std::uint8_t> base64Decode(const std::string &input);

} // namespace afterbell::oracle
