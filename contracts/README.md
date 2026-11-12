# Contracts

Afterbell's active contract is **A1**, a shared NEAR contract with isolated
logical user and agent records. It does not create per-user contracts or use a
factory. The active Rust crate is `afterbell-a1` in `shared-ledger-spike/`;
the directory name is historical and A1 is the product contract.

## A1 current source

As of 13 Nov 2026, A1 implements the shared ledger, signed NEAR/Solana/Ethereum
owner actions, NEP-141 deposits and withdrawals, agent records and policies,
market configuration, quote-authorized trading, a single EOA-signed oracle,
oracle freshness/deviation circuit breakers, isolated margin, partial and full
closes, funding, liquidation, insurance and bad-debt accounting, deferred
profit claims, LP shares with immediate withdrawals subject to pricing, minimum
output and available-liquidity checks, delayed risk increases, storage-credit
metering, and per-agent daily loss limits. The Rust test suite contains 105
tests. Findings from the three A1 review reports were reconciled against the
current source and fixes were committed.

The active contract is complete for its defined on-chain scope. Mainnet A1 has
not yet been deployed. The testnet transaction history below records earlier
contract versions; it is deployment history, not a claim that current source is
already deployed. The current constructor is `new(usdc_token, protocol_admin)`.
`protocol_admin` is a NEAR account ID and can be a single-key account controlled
by one EOA-style signer; it does not require a multisig.

The contract's delays are operation-specific: risk-increasing market and
global risk changes and oracle signer rotations wait 20 minutes; quote signer
rotations wait one hour (the selected policy); emergency pausing is immediate.
The initial signer configuration and beta-cap changes are currently immediate.
These delays govern when staged configuration becomes active, not how long an
admin transaction takes to sign or submit. A single-key admin account is
supported; the contract does not impose a blanket 20-minute delay on every
admin call.

The quote-signed path is a centrally authorized price-fill/accounting model,
not order-book or AMM matching. The dapp, C++ transaction/API wiring, production
oracle and quote services, and indexer integration remain separate deployment
and product integration work. `smart-account-spike/` and
`account-factory-spike/` are superseded experiments, not parts of A1.

## A1 shared-ledger deposit flow

A1 accepts only the configured NEP-141 USDC contract, credits a logical record
after that record has an owner, and returns unaccepted tokens to the sender. It
does not persist deposit IDs: each callback belongs to a distinct NEAR receipt,
while the ID is correlation metadata for the indexer. A1 registers an owner
key for each logical record and verifies NEAR NEP-413, Solana Ed25519, or
Ethereum EIP-191 signatures directly before withdrawing. The signed action
binds the contract, record, destination, amount, nonce, expiry, and fee cap.
Nonce and balance are updated before the asynchronous USDC transfer; a failed
transfer restores the balance while keeping the nonce consumed. Fees are
currently unsupported and must be zero.

The intended message is JSON of the form:

```json
{"account_id":"user.near","deposit_id":"unique-quote-id"}
```

Earlier A1 versions completed direct testnet withdrawals for
logical records using NEAR NEP-413, Solana Ed25519, and Ethereum EIP-191 owner
signatures. Each test deposited 1 test USDC, withdrew it from A1 to the chosen
NEAR recipient, and left that logical record at zero balance with nonce 1. The
Solana and Ethereum signatures used disposable test keys; this validated the
on-chain verification paths, not wallet-provider integrations. The current
local suite contains 105 tests, including owner signatures, accounting,
trading, oracle, LP, and agent behavior. The factory and per-user smart-account
flow below are historical experiments, not the product architecture.

### Current testnet deployment

The machine-readable deployment history, including the active A1 account per
network and state-only migrations, is [`deployments.yaml`](./deployments.yaml).
Update that file after every contract deploy or upgrade and every state
migration. Preserve prior entries; append a new record with the network,
component/release, contract account, transaction ID, UTC timestamp, WASM
SHA-256 and byte size, and source commit. For state migrations, record the
method, caller account, transaction ID, and a read-after-write verification.
After a deploy, query the live contract code hash and update the network's
`verified_code` snapshot. Use `null` only where historical records do not
contain the value.

| Component | Testnet account | Transaction |
|---|---|---|
| A1 shared ledger | `afterbellfx-dev-faucet.testnet` | [Deploy and initialize](https://explorer.testnet.near.org/transactions/5UqcRFyCmQuhPdtrVJhsVhVtBnJgdLAe1JxE9SMRDrVf) |
| A1 delayed 65% NAV risk configuration | `afterbellfx-dev-faucet.testnet` | [Upgrade](https://explorer.testnet.near.org/transactions/4nhXY6uSA881ySTfiGHgGLtbFED4cU4XiKHKaCcVsmSz) · WASM SHA-256 `369dd716a4349dd6db44fa66ff17aac79f87992e90244530180db2ca5c86e0c6` · 695,801 bytes |
| A1 immediate LP withdrawals | `afterbellfx-dev-faucet.testnet` | [Upgrade](https://explorer.testnet.near.org/transactions/9acfJpECaH71LeQ8jggmNW8owDTe1nzZcgZF92hCUuRK) · WASM SHA-256 `778906267ecaeae4b1b779f9b166c0e9c3b4d791e804df0543d220fad728ce26` · 687,234 bytes |
| A1 shared-signature upgrade | `afterbellfx-dev-faucet.testnet` | [Deploy updated code](https://explorer.testnet.near.org/transactions/7oUEY62WJ9REoxf5MEQkJR2hjr9L2ToTLpeWjfEAxfiT) |
| A1 agent-foundation upgrade | `afterbellfx-dev-faucet.testnet` | [Deploy agent-record and position reservation code](https://explorer.testnet.near.org/transactions/2Cc8rBcDRerRaehHYH7c2WaypkxrFSMR7Gxn2K3mEAyb) |
| A1 signed-price and PnL upgrade | `afterbellfx-dev-faucet.testnet` | [Deploy testnet price publisher and PnL views](https://explorer.testnet.near.org/transactions/5emN9iDxsb51KhHqBpAfq7LTY9iFAdfLfsEVoQmCEVmT) |
| A1 funded-settlement prototype | `afterbellfx-dev-faucet.testnet` | [Upgrade](https://explorer.testnet.near.org/transactions/FGhgPfAWfesmfRS4qtu9SYSn9VBYQZ5tb17J6KCLxhHQ); deployed WASM SHA-256 `ce2e1d008d16e7fc11a3d37634f41f2158c5e2261590314f30320f7f9b60c2b4` |
| A1 storage-credit and owner-rotation upgrade | `afterbellfx-dev-faucet.testnet` | [Upgrade](https://explorer.testnet.near.org/transactions/AVNMYXoHASbmDGqm4nGh3a9otqNPyqHrbKA1pxnb6vAJ); WASM SHA-256 `565aa13953f141e9bcc0ebc79c97125036cb5eb3d57a6a462cf1a8038305129e`; 423.1 KB |
| A1 legacy storage-release compatibility fix | `afterbellfx-dev-faucet.testnet` | [Upgrade](https://explorer.testnet.near.org/transactions/Ac2YyRibNc3VkJd9W8up4JpXMyXehVKvSFVCKEmXvErb); WASM SHA-256 `75accd9d413f84f0da01904c9ef2618a22160fe4626241461ab844dff6195dfe`; 423.5 KB |
| A1 protocol-admin migration support | `afterbellfx-dev-faucet.testnet` | [Upgrade](https://explorer.testnet.near.org/transactions/D3AjGHCSQpR8ir73V6ek7UxYRMkM16Axaw6g1vVgV7JC); WASM SHA-256 `3536b8e887a1c8d36a8d0b4f0e3ce4509f316ff96f2f277863d4e1c9daa3df56` |
| A1 position and limit-order flows | `afterbellfx-dev-faucet.testnet` | [Upgrade](https://explorer.testnet.near.org/transactions/BSu4huCnANr3VjdPod6jDxKHXWJ7oupmPRaoMu4vLsoh); WASM SHA-256 `cd6ca648a7265de0657b0b5a5a10afd22170001de084f2a503430fd352f05a09`; 702,772 bytes |
| Initialize protocol admin on existing A1 | `afterbellfx-dev-faucet.testnet` | [State migration](https://explorer.testnet.near.org/transactions/GN8Yqu2TEmR9icRi9tyG3M6ekRnP5KSCgVuPCN78kk4N); read view verified as `afterbellfx-dev-faucet.testnet` |
| Storage-credit funding smoke test | `near:shared-model-smoke-20261024` | [Sponsor 0.1 NEAR](https://explorer.testnet.near.org/transactions/AHAwU64UxSzJYCXCUzPq8BzXNrptbM6wjuLuVnsd2cWs); `storage_credit_of` returned `98840000000000000000000` yoctoNEAR (0.09884 NEAR) |
| Storage-credit owner withdrawal | `solana:vault-settlement-20261024` | [Sponsor 0.05 NEAR](https://explorer.testnet.near.org/transactions/CnLBTEjZeb24Y2gSpJdf5RfVduWgtGWsHFJ85oSeaJt5) · [Owner-signed withdrawal of 0.01 NEAR](https://explorer.testnet.near.org/transactions/3WM6Jgkkjnhh23MLJFr6LK7CdcZ9a2WLGnBTJAUH8VaP) · [Failed withdrawal and credit restoration](https://explorer.testnet.near.org/transactions/9KHNKbZNnPWvpUMCm6k7SGaSsNqX6fHFkXQWaWYJNSDU) |
| NEAR wallet owner path | `near:shared-model-smoke-20261024` | [Register owner](https://explorer.testnet.near.org/transactions/8AXZKPadTDchnv3GWpMFYRHwUAfWTHQd6s1nRiDTixtu) · [Deposit](https://explorer.testnet.near.org/transactions/4otFDaTMEa7BqB1V9nSfCPuKjGwSNtndvzZHEdSKRXsz) · [Direct signed withdrawal](https://explorer.testnet.near.org/transactions/4mZ2KHeRpLjGZZLn4tK8WLR9NWxqeWWdu1CBEJMAf7nH) |
| Solana wallet owner path | `solana:shared-model-smoke-20261024` | [Register owner](https://explorer.testnet.near.org/transactions/9wEqZdnU2jCNuwjebRyTNd8uN6zRcQh5Upzm6o5ZiRXe) · [Deposit](https://explorer.testnet.near.org/transactions/9Sd6eviXekP91RtRWRMj9Wq2mFbNmAqUj7FWdeNjEoa1) · [Direct signed withdrawal](https://explorer.testnet.near.org/transactions/95GArEePnTZXaKrQ57nj1miWXET2R69jCEokNBRbopHi) |
| Ethereum wallet owner path | `evm:shared-model-smoke-20261024` | [Register owner](https://explorer.testnet.near.org/transactions/9RjDvhxhj2q2iQ7yQ4LG7rn6xtCbHdMDRkURjnmArnEn) · [Deposit](https://explorer.testnet.near.org/transactions/G7Vq5irGPZNHp9EunqDStGdm69T9hQZtbx5dQ8JWkbpT) · [Direct signed withdrawal](https://explorer.testnet.near.org/transactions/3qWh56E79YcvNFsherew3Lr4Jy6vQaeuNgMqHVjFMn6g) |
| Earlier account-factory experiment | `accounts.afterbellfx-dev-faucet.testnet` | [Deploy and initialize](https://explorer.testnet.near.org/transactions/4tJDB9Nuxkzuh1Kk55mvTycrd5kzEdCyck2fXqnj2YRA) |
| Earlier per-user smart-account experiment | `alice.accounts.afterbellfx-dev-faucet.testnet` | [Create with signed owner key](https://explorer.testnet.near.org/transactions/DzNR23GL4MQPRg9pBu4nXvSh5XCL4VbUSwiJzrdeCQB3) |
| Earlier per-user smoke flow | `alice.accounts.afterbellfx-dev-faucet.testnet` | [Deposit callback](https://explorer.testnet.near.org/transactions/Do7rrFmiQXpTDuK6vKwAm81R1VsME45PdQrnx1rFcLQc) · [Withdrawal via legacy smart-account](https://explorer.testnet.near.org/transactions/eF4XNxaPzxu4d3WyH7zcs7w7AVxAXKqdAcqoSVGUQhA) |
| Circle testnet USDC | `3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af` | [Token contract on testnet](https://testnet.nearblocks.io/tokens/3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af) |

The agent foundation also completed a testnet flow using a disposable Solana
owner key and agent key. The user record received 1 USDC through the NEP-141
callback, the owner signed a 1 USDC allocation to the agent record, and the
agent signed an NVDA long with 1 USDC notional at 2x leverage. A1 reserved
0.5 USDC margin. The owner then revoked the agent, reclaiming its free 0.5
USDC, and signed a close that returned the reserved 0.5 USDC to the user
record. Final state: user record 1 USDC, agent record 0, reserved margin 0,
position absent, and agent disabled. This verifies policy and custody flows,
not market execution or PnL settlement.

The deployed A1 price version was manually exercised on testnet with a
disposable publisher key and synthetic prices. It accepted a signed NVDA price
of $100, opened a 0.5 USDC notional long at 2x with 0.25 USDC reserved margin,
accepted a signed $110 mark, and returned unrealized gross PnL of 50,000 raw
USDC units (0.05 USDC). The test position was closed using the old margin-only
path. This was not a market fill and did not realize PnL.

### Earlier testnet accounting experiments

The testnet transactions below document earlier iterations of price-aware
position settlement using synthetic prices. They do not exercise the current
local LP, funding, liquidation, oracle-quorum, or admin code. Those features
have local unit coverage only and require a fresh test deployment before
end-to-end verification. The prior testnet WASM sizes and transaction links
remain useful as deployment history, not as evidence that the current source is
deployed.

The funded settlement path completed an end-to-end test using temporary
Solana-owner and agent keys and synthetic NVDA prices. A1 accepted a 1 USDC
vault deposit, accepted a separate 2 USDC user-record deposit, allocated 1
USDC to the agent, opened a 0.5 USDC notional 2x long at $100, then owner-signed
the close at $110. It returned 0.30 USDC to the agent (0.25 margin plus 0.05
profit), debited 0.05 USDC from the vault, released all reserved payout, and
removed the position. The owner then revoked the agent and reclaimed its 1.05
USDC. Verified final state: vault 0.95 USDC with zero reserved payout, user
record 2.05 USDC, agent 0 USDC and disabled, no open position. This validates
the prototype accounting path only; it was not a live market trade.

A second funded test checked the loss path: a new agent opened a 0.5 USDC
notional 2x long at synthetic $100 and the owner signed a close at $90. A1
returned 0.20 USDC to the agent (0.25 margin less 0.05 loss), credited the
0.05 USDC loss to the vault, cleared the reservation, and removed the
position. Revocation returned the agent's remaining 0.95 USDC. Final state:
vault 1 USDC, user record 2 USDC, agent zero and disabled, no open position.

Owner-key rotation is deployed in the current testnet A1. The current and proposed
owner must both sign a domain-bound message containing the contract, logical
record, old and new keys, nonce, and expiry. A cross-chain Solana-to-Ethereum
unit test verifies the new key can withdraw afterward and the retired key
cannot. This does not provide lost-key recovery: the current key must still be
available. The current A1 review set includes owner-key rotation in its scope.

Storage credits are implemented in the current testnet A1. Anyone can call payable
`storage_deposit_for(account_id)` to sponsor NEAR; A1 charges the record's
credit for measured storage growth at `env::storage_byte_cost()`, rejects
state growth without enough credit, and returns released storage value to the
same credit. The owner can withdraw unused credit to a signed NEAR destination;
that uses the record's shared withdrawal nonce. Global market-price storage
uses a separate `protocol:market_prices` credit. Deposits to unregistered
records are refunded, and deposits that cannot pay for a new balance entry are
also refunded. Reused IDs on separate FT transfers still credit the new tokens;
the deposit ID is emitted for off-chain indexing, which should deduplicate by
NEAR receipt identity.

For upgrade compatibility, storage credits use a namespaced raw storage key
instead of changing A1's serialized state struct. The legacy processed-deposit
set remains in serialized state but is no longer read or written. Existing
records have no credit until a user or relayer tops them up; operations that
grow storage require credit. Storage releases can proceed without a prior
credit entry; A1 creates a credit entry only when the released value covers
its own storage cost. Operations with no storage delta can continue. After the
upgrade, existing testnet state loaded
successfully: the prior shared-model owner record
remained registered, its balance remained zero, and the protocol vault remained
at 1 USDC. The existing NEAR smoke record was then sponsored with 0.1 NEAR;
`storage_credit_of` returned 0.09884 NEAR after the storage-credit entry cost.
On a separate existing Solana smoke record, the owner-signed withdrawal paid
0.01 NEAR to the deployer; credit decreased by the payout and measured cost of
new withdrawal state. A second owner-signed withdrawal to a nonexistent
testnet account failed, and its callback restored the full 0.005 NEAR credit;
the view returned to 0.02774 NEAR and the nonce advanced to 3. These were
disposable test records and keys. A later fix for storage releases on legacy
records passed two additional unit cases and was deployed without reinitializing
A1. After that deployment, the vault view remained at 1 USDC and the two funded
record credits remained readable. The C++ background indexer now runs beside
the API and scans A1 account transactions using the FastNear transaction API
plus NEAR RPC receipt lookups; it does not depend on an archival NEAR endpoint.
PostgreSQL stores a durable block cursor and deduplicates events by network,
receipt ID, and log index. It has been observed scanning the current testnet
A1 history, but it indexes the deployed contract's existing event format; new
local A1 trade events still need downstream schema/API integration.

| Funded settlement testnet action | Transaction |
|---|---|
| Upgrade A1 with funded settlement | [Upgrade](https://explorer.testnet.near.org/transactions/FGhgPfAWfesmfRS4qtu9SYSn9VBYQZ5tb17J6KCLxhHQ) |
| Fund protocol vault (1 USDC) | [Vault deposit](https://explorer.testnet.near.org/transactions/FsGvyWkWFEANRAwG39fJ5txGjfwarxpfMFY5pPT1g1q) |
| Register temporary Solana test owner | [Register owner](https://explorer.testnet.near.org/transactions/GYCuXUifd4kRvzJdQjmngeg7ny2A8vqWTp2Hy7agdR5E) |
| Create agent and allocate 1 USDC | [Create agent](https://explorer.testnet.near.org/transactions/Bb7XsopQmvxfPiXUi6gCexmGbKP5VSnGsFVYp9AFzbe2) · [Allocation](https://explorer.testnet.near.org/transactions/83QAZFCzXwkyso4Xo6uLw8ezDBWR6R6jE3C5cT1wvgah) |
| Publish synthetic NVDA $100 price | [Price update](https://explorer.testnet.near.org/transactions/7eQzuDZ6a7vaZspaTUL9q5hxnfkiWZ86odBCXXQtsivk) |
| Agent-signed open at $100 | [Open position](https://explorer.testnet.near.org/transactions/9ez4CCBdvfV6GipjMMqgUc4vgCxmQWxzQsqjLjm1fPYc) |
| Publish synthetic NVDA $110 price | [Price update](https://explorer.testnet.near.org/transactions/5d5X9qqmzsWjH6kgu6RNpmkEBqnad684roqV8LkCM9no) |
| Owner-signed close and realize +0.05 USDC | [Close position](https://explorer.testnet.near.org/transactions/9zu9HvugCb2PWxJSdgazZ67LWnsGkXiJVfpvdC7qgE1s) |
| Revoke agent and return remaining balance | [Revoke agent](https://explorer.testnet.near.org/transactions/9wDBa2oYsUzvidsQgdWLnVxxb1pfmrqQgTaBizf3Jccm) |

| Loss settlement testnet action | Transaction |
|---|---|
| Create replacement temporary agent | [Create agent](https://explorer.testnet.near.org/transactions/6HsgvMutc8A1erk848W5Q2kXmeis788Y9CdT2sDwPxNQ) |
| Owner-signed allocation of 1 USDC | [Allocation](https://explorer.testnet.near.org/transactions/4eWtxKpxF5FFg3sm1Rp7JYBtcuP12eoyEUrRMvb4kooL) |
| Publish synthetic NVDA $100 mark | [Price update](https://explorer.testnet.near.org/transactions/67XtHBdHKfDytKm5SFRHaJ1rSjxjDayPok37EmpckANh) |
| Agent-signed position open | [Open position](https://explorer.testnet.near.org/transactions/8Vv5KxykumJ4i4KqqakNCYVzMHUC3vNBFvqAEaG3yPSr) |
| Publish synthetic NVDA $90 mark | [Price update](https://explorer.testnet.near.org/transactions/AVoBjNYR8xWRnqpLivcFwePZNVnQnLCAdesMYAHVDSC3) |
| Owner-signed close and realize -0.05 USDC | [Close position](https://explorer.testnet.near.org/transactions/HVcoR1kF6sQWr6g4BnPA94EfF4u6nf7iRm74AxCYk9fX) |
| Revoke agent and return remaining balance | [Revoke agent](https://explorer.testnet.near.org/transactions/EQXuDWw9d5xb7QcWSzdmi2ikspUVXShuPoZ3pEtWYaPg) |

| Agent testnet action | Transaction |
|---|---|
| Upgrade A1 with agent foundation | [Upgrade](https://explorer.testnet.near.org/transactions/2Cc8rBcDRerRaehHYH7c2WaypkxrFSMR7Gxn2K3mEAyb) |
| Register disposable Solana test owner | [Register owner](https://explorer.testnet.near.org/transactions/Dt4Dbt7kb4VBJ9vUPiN28M56MzshVLFspunfMfttSFTW) |
| Create linked agent record and policy | [Create agent](https://explorer.testnet.near.org/transactions/HYXMLPb4Dw972rFMhcAogDBwNgy62h2PFS25FagRE6DM) |
| Deposit 1 USDC into user record | [Deposit](https://explorer.testnet.near.org/transactions/BC3UyxXsRHkmuLYiupQ473RQGBFL1yhhmyMnTxqSPUTv) |
| Owner-signed allocation to agent record | [Fund agent](https://explorer.testnet.near.org/transactions/2hNN1pku88CnF3TejYiMggQWkZHinHVQ4LDcfqHALKTP) |
| Agent-signed NVDA position open | [Open position](https://explorer.testnet.near.org/transactions/Bqa143tGZRvMxtSsLBjghMXmQGomA56v6t7rigXGCtHG) |
| Owner revokes agent and reclaims free funds | [Revoke agent](https://explorer.testnet.near.org/transactions/8q3FBf7bzSUcCfJdBRptxWm5Vkdx3aARRpdt9Ry4rdk6) |
| Owner-signed close after revocation | [Close position](https://explorer.testnet.near.org/transactions/3hqMoLq3a5tBaQXqqDT7WaRz25MQSscmDjUE4asdMVCc) |
| Upgrade A1 with signed-price and PnL path | [Upgrade](https://explorer.testnet.near.org/transactions/5emN9iDxsb51KhHqBpAfq7LTY9iFAdfLfsEVoQmCEVmT) |
| Create temporary price-test agent | [Create agent](https://explorer.testnet.near.org/transactions/4ipTRw9oJ22ufm59QBxCSNZ6NrFMSQAq84JoS1p2mGad) |
| Publish signed synthetic NVDA $100 price | [Price update 1](https://explorer.testnet.near.org/transactions/CuiuJhv8MYZoFsFDLriQPPZ9p5PmkoSbHxstLd3M284f) |
| Owner allocation to price-test agent | [Fund agent](https://explorer.testnet.near.org/transactions/HZke7VRa4hzN7SSTVnUeuVzhCFvLRYf4XgDSPjYD52ef) |
| Agent opens price-bound NVDA long | [Open position](https://explorer.testnet.near.org/transactions/9Ns6Cf3X9irbBaf4F3htiF2cSLGVUUe9guUVrmKz3Y9f) |
| Publish signed synthetic NVDA $110 price | [Price update 2](https://explorer.testnet.near.org/transactions/BjsYkzfGgkkaz8va6mXtr5ssYfDatwp4J57aouGQ4Jzv) |
| Close test position after PnL query | [Close position](https://explorer.testnet.near.org/transactions/CS7XX1HEFtx9qbUQSYgG35Jxr3A3KUaSEWm4Ma9Yihoq) |
| Revoke temporary price-test agent | [Revoke agent](https://explorer.testnet.near.org/transactions/2zCEeASkL3Pqd6XxHMKbZssLjHMiwcAHw11U4iNyB1aH) |

The funded-settlement test used disposable testnet records and synthetic
prices. The protocol-vault and temporary user-record balances are listed
above; no mainnet funds or accounts were touched.

## Historical account-factory spike (not the product model)

`account-factory-spike/` pins the smart-account WASM and A1 contract address at
factory initialization. A creation request must name a direct child account,
include an unexpired owner signature over the factory, new account ID, A1
address, and owner key, and attach at least the configured minimum NEAR
deposit. The factory then creates the child account, funds it, deploys the
pinned code, and calls its initializer in one atomic promise batch. If any
action fails, NEAR rolls back the batch. Creation proofs support NEAR NEP-413,
Solana Ed25519, and Ethereum EIP-191.

The factory is deployed on testnet and is not deployed on mainnet. It exists as
a historical prototype from the per-user-account design exploration. The chosen product
architecture has no account factory; the factory and per-user smart-account
code should not be extended as product components. A1's shared-contract
signature and withdrawal path is verified on testnet. A1 is the active product
contract; the factory is not part of its architecture.



## Earlier agent logical-record implementation records

A user can create one linked agent record with a wallet-signed capability
policy: allowed market ids, maximum position size, maximum leverage (1x–5x),
maximum USDC allocation, and an expiry of up to 30 days. Agent management uses
a separate per-user nonce. The user can move USDC ledger credit into that
record with another signed action, then revoke the agent and return its
remaining ledger balance to the user record. A revoked agent can be replaced
by a new record. Agent records have no registered external owner; owner
registration and the external withdrawal path reject them.

The earlier deployed A1 agent version verified short-lived agent-signed open actions against the stored market,
per-position notional, aggregate open-notional, leverage, expiry, and allocation
limits. The original `open_agent_position` path reserves margin without an
entry price and is a historical reservation-only path. The additional
`open_agent_market_position` path requires a fresh signed price, an agent-signed
reference price/sequence/slippage bound, and stores the accepted entry mark.
The reservation-only path has no entry price or PnL settlement. Current source
capabilities are described at the top of this document; historical testnet
deployments do not include all current changes.

A1 now also exposes `calculate_linear_pnl`, an indicative view that accepts a
notional, entry price, mark price, and side and returns gross PnL in raw USDC
units using `floor(notional * abs(mark - entry) / entry)`. This view does not
authenticate prices, read a stored position, charge fees or funding, or change
balances. It is a math helper only; it cannot be used as evidence of a trade
or as settlement. Price-aware agent positions instead use `agent_position_pnl`
to calculate unrealized PnL from their stored entry and latest accepted mark.

An earlier A1 testnet version had a signed-price path. A single
compiled-in Ed25519 publisher key authorizes updates bound to the contract,
market, price, observation time, and monotonically increasing sequence. The
price-aware agent-open method signs a reference price and slippage ceiling,
uses the latest fresh accepted price as entry, and exposes unrealized gross
PnL from that mark. The publisher private key is local-only under ignored
`contracts/target/oracle-testnet/`; it must never be committed. This deployed
price/PnL version passed a WASM-target cargo check, 47 local unit tests, and a
manual testnet price/PnL exercise. These entries document earlier iterations;
they do not describe the current local A1 source or its mainnet deployment.
