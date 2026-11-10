# Afterbell

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

The backend provides a read-only HTTP API and a NEAR deposit indexer. The API currently exposes health checks and the Robinhood Chain market registry; markets are explicitly marked development-only and cannot be traded yet. Set `INDEXER_ENABLED=true` to run the indexer on a managed background thread inside the API process, allowing both to run in one Render service. The indexer also remains available as a standalone executable for local development and operations.

On macOS, install the build dependencies with Homebrew:

```sh
brew install cmake pkg-config postgresql@17 curl jsoncpp drogon
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

Copy `backend/.env.example` to `backend/.env` or set these variables in the process environment. Keep the existing `.env` private. `NEAR_NETWORK`, `INDEXER_NEAR_RPC_URL`, `INDEXER_TX_API_URL`, `A1_CONTRACT_ID`, `NEAR_USDC_TOKEN_ID`, and `DATABASE_URL` are required. `INDEXER_START_BLOCK` defaults to A1's testnet deployment block (`274456124`) for the first run; subsequent runs use the database cursor. Change it for another deployment/network or to backfill from a later point. The worker checks RPC `chain_id` against `NEAR_NETWORK` before opening its database connection. `INDEXER_FINALITY_LAG_BLOCKS` controls how far behind the finalized head to query; `INDEXER_RESCAN_OVERLAP_BLOCKS` rechecks recent history to tolerate FastNear indexing delay. `INDEXER_POLL_INTERVAL_MS` controls polling frequency.

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
| API and indexer | default image command (`./afterbell-api`) | `/health/ready` | `DATABASE_URL`, `NEAR_NETWORK`, `A1_CONTRACT_ID`, `MARKET_REGISTRY_PATH=/app/backend/rh_stocks_addresses.json`, `INDEXER_ENABLED=true`, `NEAR_USDC_TOKEN_ID`, `INDEXER_NEAR_RPC_URL`, `INDEXER_TX_API_URL`, `INDEXER_START_BLOCK` on first run |

Render supplies `PORT`. The indexer uses FastNear's testnet transaction API to discover A1 history without a full archival NEAR RPC scan. Configure `INDEXER_FINALITY_LAG_BLOCKS`, `INDEXER_RESCAN_OVERLAP_BLOCKS`, and `INDEXER_POLL_INTERVAL_MS` as needed. Do not set `INDEXER_START_BLOCK` earlier than the deployed A1 instance. The existing standalone `./afterbell-indexer` command is available if we later decide to run the worker as its own Render service.
