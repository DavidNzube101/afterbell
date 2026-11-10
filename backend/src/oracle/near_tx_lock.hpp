#pragma once

#include <mutex>

namespace afterbell::oracle {

// Oracle updates and trade relays share a NEAR access key. Serialize nonce
// reads and broadcasts across both workers in this process.
inline std::mutex &nearTransactionMutex() {
    static std::mutex mutex;
    return mutex;
}

} // namespace afterbell::oracle
