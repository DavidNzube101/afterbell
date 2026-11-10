#include "oracle/crypto.hpp"

#include <openssl/bn.h>
#include <openssl/ec.h>
#include <openssl/evp.h>
#include <openssl/obj_mac.h>
#include <openssl/sha.h>

#include <algorithm>
#include <array>
#include <cctype>
#include <memory>
#include <stdexcept>

namespace afterbell::oracle {
namespace {
constexpr std::array<std::uint64_t, 24> RC{
    0x0000000000000001ULL,0x0000000000008082ULL,0x800000000000808aULL,0x8000000080008000ULL,
    0x000000000000808bULL,0x0000000080000001ULL,0x8000000080008081ULL,0x8000000000008009ULL,
    0x000000000000008aULL,0x0000000000000088ULL,0x0000000080008009ULL,0x000000008000000aULL,
    0x000000008000808bULL,0x800000000000008bULL,0x8000000000008089ULL,0x8000000000008003ULL,
    0x8000000000008002ULL,0x8000000000000080ULL,0x000000000000800aULL,0x800000008000000aULL,
    0x8000000080008081ULL,0x8000000000008080ULL,0x0000000080000001ULL,0x8000000080008008ULL};
constexpr std::array<unsigned, 24> ROT{1,3,6,10,15,21,28,36,45,55,2,14,27,41,56,8,25,43,62,18,39,61,20,44};
constexpr std::array<unsigned, 24> PI{10,7,11,17,18,3,5,16,8,21,24,4,15,23,19,13,12,2,20,14,22,9,6,1};
using BnPtr = std::unique_ptr<BIGNUM, decltype(&BN_free)>;
using CtxPtr = std::unique_ptr<BN_CTX, decltype(&BN_CTX_free)>;
using GroupPtr = std::unique_ptr<EC_GROUP, decltype(&EC_GROUP_free)>;
using PointPtr = std::unique_ptr<EC_POINT, decltype(&EC_POINT_free)>;
using KeyPtr = std::unique_ptr<EC_KEY, decltype(&EC_KEY_free)>;
using SigPtr = std::unique_ptr<ECDSA_SIG, decltype(&ECDSA_SIG_free)>;

std::vector<std::uint8_t> hexDecode(std::string text) {
    if (text.rfind("0x", 0) == 0 || text.rfind("0X", 0) == 0) text.erase(0, 2);
    if (text.size() != 64 || !std::all_of(text.begin(), text.end(), [](unsigned char c) { return std::isxdigit(c); }))
        throw std::invalid_argument("oracle EVM private key must be 32-byte hex");
    std::vector<std::uint8_t> result(32);
    for (std::size_t i = 0; i < result.size(); ++i) result[i] = static_cast<std::uint8_t>(std::stoul(text.substr(i * 2, 2), nullptr, 16));
    return result;
}

std::array<std::uint8_t, 65> publicKey(const std::string &privateKeyHex) {
    const auto raw = hexDecode(privateKeyHex);
    GroupPtr group(EC_GROUP_new_by_curve_name(NID_secp256k1), EC_GROUP_free);
    CtxPtr ctx(BN_CTX_new(), BN_CTX_free);
    BnPtr secret(BN_bin2bn(raw.data(), static_cast<int>(raw.size()), nullptr), BN_free);
    PointPtr point(group ? EC_POINT_new(group.get()) : nullptr, EC_POINT_free);
    std::array<std::uint8_t, 65> out{};
    if (!group || !ctx || !secret || !point ||
        EC_POINT_mul(group.get(), point.get(), secret.get(), nullptr, nullptr, ctx.get()) != 1 ||
        EC_POINT_point2oct(group.get(), point.get(), POINT_CONVERSION_UNCOMPRESSED, out.data(), out.size(), ctx.get()) != out.size())
        throw std::runtime_error("could not derive oracle signer public key");
    return out;
}

bool recoveredKeyMatches(const EC_GROUP *group, BN_CTX *ctx, const BIGNUM *r,
                         const BIGNUM *s, const std::uint8_t digest[32], int parity,
                         const std::array<std::uint8_t, 65> &expected) {
    BnPtr order(BN_new(), BN_free), x(BN_dup(r), BN_free), e(BN_new(), BN_free), z(BN_bin2bn(digest, 32, nullptr), BN_free);
    BnPtr rinv(BN_new(), BN_free), u1(BN_new(), BN_free), u2(BN_new(), BN_free);
    PointPtr R(EC_POINT_new(group), EC_POINT_free), Q(EC_POINT_new(group), EC_POINT_free), check(EC_POINT_new(group), EC_POINT_free);
    if (!order || !x || !e || !z || !rinv || !u1 || !u2 || !R || !Q || !check ||
        EC_GROUP_get_order(group, order.get(), ctx) != 1 ||
        EC_POINT_set_compressed_coordinates(group, R.get(), x.get(), parity, ctx) != 1 ||
        EC_POINT_mul(group, check.get(), nullptr, R.get(), order.get(), ctx) != 1 || !EC_POINT_is_at_infinity(group, check.get())) return false;
    if (BN_mod(z.get(), z.get(), order.get(), ctx) != 1 || BN_mod_sub(e.get(), order.get(), z.get(), order.get(), ctx) != 1) return false;
    if (!BN_mod_inverse(rinv.get(), r, order.get(), ctx) ||
        BN_mod_mul(u1.get(), e.get(), rinv.get(), order.get(), ctx) != 1 ||
        BN_mod_mul(u2.get(), s, rinv.get(), order.get(), ctx) != 1 ||
        EC_POINT_mul(group, Q.get(), u1.get(), R.get(), u2.get(), ctx) != 1) return false;
    std::array<std::uint8_t, 65> actual{};
    return EC_POINT_point2oct(group, Q.get(), POINT_CONVERSION_UNCOMPRESSED, actual.data(), actual.size(), ctx) == actual.size() && actual == expected;
}

} // namespace

std::array<std::uint8_t, 32> keccak256(const std::uint8_t *input, std::size_t size) {
    constexpr std::size_t rate = 136;
    std::array<std::uint64_t, 25> state{};
    std::size_t offset = 0;
    while (size - offset >= rate) {
        for (std::size_t i = 0; i < rate / 8; ++i) {
            std::uint64_t lane = 0;
            for (unsigned j = 0; j < 8; ++j) lane |= std::uint64_t(input[offset + i * 8 + j]) << (8 * j);
            state[i] ^= lane;
        }
        offset += rate;
        for (auto rc : RC) {
            std::array<std::uint64_t, 5> c{}, d{};
            for (unsigned x = 0; x < 5; ++x) c[x] = state[x] ^ state[x+5] ^ state[x+10] ^ state[x+15] ^ state[x+20];
            for (unsigned x = 0; x < 5; ++x) d[x] = c[(x+4)%5] ^ ((c[(x+1)%5] << 1) | (c[(x+1)%5] >> 63));
            for (unsigned y = 0; y < 5; ++y) for (unsigned x = 0; x < 5; ++x) state[x+5*y] ^= d[x];
            auto current = state[1];
            for (unsigned i = 0; i < 24; ++i) { const auto next = state[PI[i]]; state[PI[i]] = (current << ROT[i]) | (current >> (64-ROT[i])); current = next; }
            for (unsigned y = 0; y < 5; ++y) { std::array<std::uint64_t,5> row{}; for (unsigned x=0;x<5;++x) row[x]=state[x+5*y]; for (unsigned x=0;x<5;++x) state[x+5*y]=row[x]^((~row[(x+1)%5])&row[(x+2)%5]); }
            state[0] ^= rc;
        }
    }
    std::array<std::uint8_t, rate> tail{};
    const auto remaining = size - offset;
    std::copy(input + offset, input + size, tail.begin());
    tail[remaining] = 0x01;
    tail[rate - 1] |= 0x80;
    for (std::size_t i = 0; i < rate / 8; ++i) {
        std::uint64_t lane = 0;
        for (unsigned j = 0; j < 8; ++j) lane |= std::uint64_t(tail[i*8+j]) << (8*j);
        state[i] ^= lane;
    }
    for (auto rc : RC) {
        std::array<std::uint64_t,5> c{},d{};
        for(unsigned x=0;x<5;++x)c[x]=state[x]^state[x+5]^state[x+10]^state[x+15]^state[x+20];
        for(unsigned x=0;x<5;++x)d[x]=c[(x+4)%5]^((c[(x+1)%5]<<1)|(c[(x+1)%5]>>63));
        for(unsigned y=0;y<5;++y)for(unsigned x=0;x<5;++x)state[x+5*y]^=d[x];
        auto current=state[1]; for(unsigned i=0;i<24;++i){const auto next=state[PI[i]];state[PI[i]]=(current<<ROT[i])|(current>>(64-ROT[i]));current=next;}
        for(unsigned y=0;y<5;++y){std::array<std::uint64_t,5> row{};for(unsigned x=0;x<5;++x)row[x]=state[x+5*y];for(unsigned x=0;x<5;++x)state[x+5*y]=row[x]^((~row[(x+1)%5])&row[(x+2)%5]);}
        state[0]^=rc;
    }
    std::array<std::uint8_t,32> out{};
    for(std::size_t i=0;i<out.size();++i)out[i]=static_cast<std::uint8_t>(state[i/8]>>(8*(i%8)));
    return out;
}

std::array<std::uint8_t,32> sha256(const std::uint8_t *data, std::size_t size) {
    std::array<std::uint8_t,32> out{};
    if (!SHA256(data, size, out.data())) throw std::runtime_error("SHA-256 failed");
    return out;
}

std::string ethereumAddress(const std::string &privateKeyHex) {
    const auto pub = publicKey(privateKeyHex);
    const auto hash = keccak256(pub.data()+1, 64);
    static constexpr char digits[] = "0123456789abcdef";
    std::string out = "0x";
    for (std::size_t i=12;i<32;++i) { out.push_back(digits[hash[i]>>4]); out.push_back(digits[hash[i]&15]); }
    return out;
}

std::vector<std::uint8_t> eip191Sign(const std::string &message, const std::string &privateKeyHex) {
    const std::string personal = std::string(1, '\x19') + "Ethereum Signed Message:\n" + std::to_string(message.size()) + message;
    const auto digest = keccak256(reinterpret_cast<const std::uint8_t *>(personal.data()), personal.size());
    const auto expected = publicKey(privateKeyHex);
    const auto raw = hexDecode(privateKeyHex);
    KeyPtr key(EC_KEY_new_by_curve_name(NID_secp256k1), EC_KEY_free);
    BnPtr secret(BN_bin2bn(raw.data(), static_cast<int>(raw.size()), nullptr), BN_free);
    CtxPtr ctx(BN_CTX_new(), BN_CTX_free);
    GroupPtr group(EC_GROUP_new_by_curve_name(NID_secp256k1), EC_GROUP_free);
    if (!key || !secret || !ctx || !group || EC_KEY_set_private_key(key.get(), secret.get()) != 1)
        throw std::runtime_error("could not initialize EVM signer");
    SigPtr signature(ECDSA_do_sign(digest.data(), static_cast<int>(digest.size()), key.get()), ECDSA_SIG_free);
    if (!signature) throw std::runtime_error("EVM signature failed");
    const BIGNUM *r = nullptr, *s = nullptr;
    ECDSA_SIG_get0(signature.get(), &r, &s);
    BnPtr order(BN_new(), BN_free), half(BN_new(), BN_free), lowS(BN_dup(s), BN_free);
    if (!order || !half || !lowS || EC_GROUP_get_order(group.get(), order.get(), ctx.get()) != 1) throw std::runtime_error("could not normalize EVM signature");
    BN_rshift1(half.get(), order.get());
    bool flipped = BN_cmp(lowS.get(), half.get()) > 0;
    if (flipped) BN_sub(lowS.get(), order.get(), lowS.get());
    std::vector<std::uint8_t> out(65);
    if (BN_bn2binpad(r, out.data(), 32) != 32 || BN_bn2binpad(lowS.get(), out.data()+32, 32) != 32) throw std::runtime_error("EVM signature serialization failed");
    int parity = -1;
    for (int candidate=0;candidate<2;++candidate) if (recoveredKeyMatches(group.get(), ctx.get(), r, lowS.get(), digest.data(), candidate, expected)) { parity=candidate; break; }
    if (parity < 0) throw std::runtime_error("could not derive EVM signature recovery id");
    out[64] = static_cast<std::uint8_t>(parity ^ (flipped ? 1 : 0));
    return out;
}

std::vector<std::uint8_t> base58Decode(const std::string &input) {
    static constexpr char alphabet[] = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    std::vector<std::uint8_t> out(1, 0);
    for (char c : input) {
        const char *p = std::find(std::begin(alphabet), std::end(alphabet)-1, c);
        if (p == std::end(alphabet)-1) throw std::invalid_argument("invalid base58 data");
        unsigned carry = static_cast<unsigned>(p-alphabet);
        for (auto it=out.rbegin();it!=out.rend();++it) { carry += 58U * *it; *it=static_cast<std::uint8_t>(carry); carry >>= 8; }
        while (carry) { out.insert(out.begin(),static_cast<std::uint8_t>(carry)); carry >>= 8; }
    }
    std::size_t zeros=0; while(zeros<input.size()&&input[zeros]=='1')++zeros;
    auto first=out.begin(); while(first!=out.end()&&*first==0)++first;
    std::vector<std::uint8_t> result(zeros,0); result.insert(result.end(),first,out.end()); return result;
}

std::string base58Encode(const std::uint8_t *data, std::size_t size) {
    static constexpr char alphabet[] = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    std::vector<std::uint8_t> digits(1,0);
    for(std::size_t i=0;i<size;++i){unsigned carry=data[i];for(auto it=digits.rbegin();it!=digits.rend();++it){carry+=256U**it;*it=static_cast<std::uint8_t>(carry%58);carry/=58;}while(carry){digits.insert(digits.begin(),static_cast<std::uint8_t>(carry%58));carry/=58;}}
    std::size_t zeros=0;while(zeros<size&&data[zeros]==0)++zeros;
    std::string out(zeros,'1'); auto first=digits.begin(); while(first+1!=digits.end()&&*first==0)++first; for(;first!=digits.end();++first)out.push_back(alphabet[*first]); return out;
}

std::string base64Encode(const std::uint8_t *data, std::size_t size) {
    std::string out(4*((size+2)/3),'\0');
    if (EVP_EncodeBlock(reinterpret_cast<unsigned char *>(out.data()), data, static_cast<int>(size)) < 0) throw std::runtime_error("base64 encode failed");
    return out;
}

std::vector<std::uint8_t> base64Decode(const std::string &input) {
    std::vector<std::uint8_t> out(3*(input.size()/4)+3);
    const auto n=EVP_DecodeBlock(out.data(),reinterpret_cast<const unsigned char*>(input.data()),static_cast<int>(input.size()));
    if(n<0)throw std::invalid_argument("invalid base64 data");
    std::size_t size=static_cast<std::size_t>(n); if(!input.empty()&&input.back()=='=')--size;if(input.size()>1&&input[input.size()-2]=='=')--size;out.resize(size);return out;
}

} // namespace afterbell::oracle
