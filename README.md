![Afterbell project banner](docs/assets/afterbell-banner.png)

# Afterbell

[![CI](https://github.com/DavidNzube101/afterbell/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/DavidNzube101/afterbell/actions/workflows/ci.yml)
![C++17](https://img.shields.io/badge/C%2B%2B-17-00599C?logo=cplusplus&logoColor=white)
![Rust](https://img.shields.io/badge/Rust-contracts-000000?logo=rust&logoColor=white)
![React](https://img.shields.io/badge/React-19-149ECA?logo=react&logoColor=white)
![TanStack Router](https://img.shields.io/badge/TanStack-Router-FF4154?logo=reactquery&logoColor=white)
![Bun](https://img.shields.io/badge/Bun-1.4-f9f1e1?logo=bun&logoColor=000000)
![NEAR Protocol](https://img.shields.io/badge/Chain-NEAR%20Protocol-00C08B?logo=near&logoColor=white)

Afterbell is the open source trading dapp monorepo. The public landing page remains in `../landing`; it calls the separate landing server documented in [`openapi.yaml`](./openapi.yaml). The trading frontend lives at this repository root, with C++ workers in `backend/` and NEAR contracts in `contracts/`.

## Repository layout

```text
app/
├── backend/       # C++ off-chain services, including the NEAR deposit indexer
├── contracts/     # NEAR smart contracts
├── openapi.yaml   # Public HTTP API contract
└── README.md
```

## Dapp frontend

The TanStack React dapp is served from this repository root. Its API requests use
`/api` paths: Vite proxies them to `VITE_API_PROXY_TARGET` during local
development, while Vercel forwards them to `https://api.afterbellfx.xyz`. This
keeps browser requests same-origin and avoids requiring cross-origin API access.

For local frontend development, run the API on port `8080`, then:

```sh
bun install
bun run dev
```

To point the local dapp at the deployed API instead, copy `.env.example` to
`.env.local` and set `VITE_API_PROXY_TARGET=https://api.afterbellfx.xyz`.
Production Vercel settings use the repository root, `bun run build`, and `dist`
as the output directory. Add `testnet.afterbellfx.xyz` to the Vercel project's
Domains and configure the DNS record Vercel provides.

## C++ API and workers

The backend provides a read-only HTTP API, a NEAR deposit indexer, and an optional Robinhood Chain price publisher. Markets are explicitly marked development-only and cannot be traded yet. Set `INDEXER_ENABLED=true` to run the indexer and `ORACLE_ENABLED=true` to run the price publisher as managed background threads inside the API process, allowing the API and workers to share one Render service. The indexer also remains available as a standalone executable for local development and operations.

On macOS, install the build dependencies with Homebrew:

```sh
brew install cmake pkg-config postgresql@17 curl jsoncpp drogon openssl@3
```

Keep `backend/.env` private. From the `app/` root, build and run the existing indexer tests, then start either service:

```sh
export CMAKE_PREFIX_PATH="$(brew --prefix postgresql@17):$(brew --prefix curl):$(brew --prefix jsoncpp)"
make build
ctest --test-dir backend/build --output-on-failure
make api
make indexer
```

`make api` and `make indexer` stay attached to their processes; stop either with Ctrl-C. The API listens on `PORT` (default `8080`) and needs `DATABASE_URL` for its readiness check. Its `GET /v1/markets` endpoint reads `backend/rh_stocks_addresses.json`. Set `INDEXER_ENABLED=true` to run indexing in the API process. Render logs include a structured startup event and one JSON record per HTTP request, with request ID, method, path, status, and duration; query values are omitted. The indexer uses libpq parameterized queries and explicit transactions, which work through a PostgreSQL transaction pooler without named prepared statements.

## NEAR deposit indexer

`backend/` includes a C++ A1 deposit indexer. It asks FastNear's indexed account API for transactions associated with A1, fetches their receipt outcomes in batches, filters NEAR USDC `ft_transfer_call` transactions, and stores successful A1 `deposit_credited` logs. This avoids downloading every historical block from an archival RPC. Each event is keyed by `(network, receipt_id, log_index)`, so a repeated `deposit_id` in a later token transfer is recorded as a separate deposit. Event inserts and the scanned-height cursor commit in one PostgreSQL transaction; a restart resumes from the cursor with a configurable overlap to catch indexing delays. A1 remains the source of truth for balances; FastNear is only the off-chain event discovery source.

Copy `backend/.env.example` to `backend/.env` or set these variables in the process environment. Keep the existing `.env` private. `NEAR_NETWORK`, its matching RPC URL (`NEAR_TESTNET_RPC_URL` for testnet or `NEAR_RPC_URL` for mainnet), `INDEXER_TX_API_URL`, `A1_CONTRACT_ID`, `NEAR_USDC_TOKEN_ID`, and `DATABASE_URL` are required when the indexer runs. Both the API's background workers and standalone indexer select the endpoint from `NEAR_NETWORK` and verify the RPC chain ID. `INDEXER_START_BLOCK` defaults to A1's testnet deployment block (`274456124`) for the first run; subsequent runs use the database cursor. Change it for another deployment/network or to backfill from a later point. `INDEXER_FINALITY_LAG_BLOCKS` controls how far behind the finalized head to query; `INDEXER_RESCAN_OVERLAP_BLOCKS` rechecks recent history to tolerate FastNear indexing delay. `INDEXER_POLL_INTERVAL_MS` controls polling frequency.

### Robinhood oracle publisher

The optional publisher reads each listed stock's Uniswap V3 stock/USDG pool on Robinhood Chain (chain ID `4663`), computes a 5-minute `observe()` TWAP and current spot tick, signs A1's EIP-191 `OracleAttestation` with `ORACLE_SIGNER_PRIVATE_KEY`, and submits it to A1 using the NEAR account in `ORACLE_RELAYER_ACCOUNT_ID`. Its signer address must already be configured in A1 with `configure_oracle_signer`. The publisher checks the NEAR RPC network, the configured A1 signer, each pool's factory registration and exact stock/USDG token pair, active liquidity, sample freshness, and A1's 50-bps spot/TWAP bound. It publishes on a 10-bps price move or a 45-second heartbeat, avoiding NEAR gas for unchanged prices.

The verified pool list is in [`backend/rh_uniswap_pools.json`](backend/rh_uniswap_pools.json). Pools are selected from live factory lookups and ranked by USDG liquidity, cross-checked against Floatsheets' NVDA and TSLA markets. The quote token is Robinhood USDG, so this oracle assumes USDG remains worth one US dollar; that peg must be monitored before enabling real trading. The publisher does not send transactions or pay gas on Robinhood Chain. `ROBINHOOD_RPC_URL` is used for reads only; NEAR gas comes from the dedicated relayer account. Testnet currently uses `afterbellfx-oracle.testnet`, with its key in the ignored `contracts/oracle-publisher/` directory. The account was faucet-created with 10 NEAR for testing.

Leave `ORACLE_ENABLED=false` until A1 has its EVM signer configured and the Render service has `ROBINHOOD_RPC_URL`, the matching network RPC URL (`NEAR_TESTNET_RPC_URL` or `NEAR_RPC_URL`), `ORACLE_RELAYER_ACCOUNT_ID`, `ORACLE_RELAYER_PRIVATE_KEY`, and `ORACLE_SIGNER_PRIVATE_KEY`. Never put either private key in Git. Oracle state and recent per-market sampling/publishing status are exposed at `GET /health/status` and `GET /health/ready`.

Build and run from the `app/` directory:

```sh
make build
ctest --test-dir backend/build --output-on-failure
make indexer
```

The indexer creates `near_indexer_cursors` and `near_deposit_events` if they do not exist. It does not write user balances or replay-suppress deposits; A1 remains the source of truth for credited funds. FastNear is a third-party index source, so reconcile the event table against A1 state before using it for reporting or accounting.

## Container deployment

GitHub Actions follows the landing-server pipeline: lint, unit tests, build both binaries, build the container, and publish `ghcr.io/davidnzube101/afterbell` on pushes to `main`. Pull requests build the image without publishing it. Set the GitHub Actions secret `RENDER_DEPLOY_HOOK_URL` to trigger a Render deploy after a successful publish. Make the GHCR package public or configure Render with credentials to pull a private package.

Create one Render **Docker Web Service** from the GHCR image. The default command runs the API; set `INDEXER_ENABLED=true` so its C++ indexer runs in the background in that same service:

| Service | Start command | Health check | Required environment |
|---|---|---|---|
| API and indexer | default image command (`./afterbell-api`) | `/health/ready` | `DATABASE_URL`, `NEAR_NETWORK`, matching `NEAR_TESTNET_RPC_URL` or `NEAR_RPC_URL`, `A1_CONTRACT_ID`, `MARKET_REGISTRY_PATH=/app/backend/rh_stocks_addresses.json`, `INDEXER_ENABLED=true`, `NEAR_USDC_TOKEN_ID`, `INDEXER_TX_API_URL`, `INDEXER_START_BLOCK` on first run |

Render supplies `PORT`. The indexer uses FastNear's testnet transaction API to discover A1 history without a full archival NEAR RPC scan. Configure `INDEXER_FINALITY_LAG_BLOCKS`, `INDEXER_RESCAN_OVERLAP_BLOCKS`, and `INDEXER_POLL_INTERVAL_MS` as needed. Do not set `INDEXER_START_BLOCK` earlier than the deployed A1 instance. The existing standalone `./afterbell-indexer` command is available if we later decide to run the worker as its own Render service.

To enable price publishing later, also set `ORACLE_ENABLED=true`, `ROBINHOOD_RPC_URL`, the matching network RPC URL (`NEAR_TESTNET_RPC_URL` or `NEAR_RPC_URL`), `ORACLE_RELAYER_ACCOUNT_ID`, `ORACLE_RELAYER_PRIVATE_KEY`, and `ORACLE_SIGNER_PRIVATE_KEY` in Render. Do not turn it on until `oracle_signer()` on the target A1 contract returns the EVM address paired with the EVM private key.
