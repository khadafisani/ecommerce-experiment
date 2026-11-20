# Tosca Market

A compact Indonesian marketplace with a Svelte storefront and separate merchant and admin workspaces.

| Folder | Contents |
| --- | --- |
| `frontend/` | Svelte 5.58.0, Bun 1.4.4, Vite, UI assets, and Bun lockfile |
| `backend/` | Rust 1.100.0, Axum 0.8.9, MariaDB migrations, demo fixtures, and backend configuration |

## Run locally

The backend uses the already-running MariaDB and Meilisearch services at `127.0.0.1:3306` and `127.0.0.1:7700` by default. Development credentials are `root/root` for MariaDB and no API key for Meilisearch. The application creates the named database if needed and applies migrations on startup.

1. From `backend/`, copy `.env.example` to `.env`. Set `JWT_SECRET` to a randomly generated value of at least 32 bytes. In PowerShell, `[guid]::NewGuid().ToString() + [guid]::NewGuid().ToString()` produces a value for local development.
2. Set `ADMIN_EMAIL` and `ADMIN_PASSWORD` to seed the initial administrator. Set `DEMO_CATALOG=true` for a sample storefront. Set `MANUAL_PAYMENT_INSTRUCTIONS` to the instructions buyers should see after ordering.
3. In `backend/`, run `cargo run`.
4. In a second terminal, run `bun install` and `bun run dev` from `frontend/`. Open [the development storefront](http://127.0.0.1:5173). Alternatively, `bun run build` produces the frontend that Axum serves at [127.0.0.1:8080](http://127.0.0.1:8080).

## Seed the test marketplace

With `backend/.env` configured for your local database, run this from `backend/`.

```powershell
cargo run --locked -- --seed-test
```

If the API is already running on Windows, use `cargo run --locked --target-dir target/seed-tool -- --seed-test` to build the seed command separately.

The seed adds 24 shops, 768 products, 2,304 variants, 240 buyers, 48 shop employees, and 10,000 checkouts. Purchase dates span about four years, with more activity in recent months. Seeded listings display product initials in place of fixture photos. The original checkout scenarios, tracking, reviews, returns, notifications, and moderation queues are included. An existing 1,800-checkout seed receives 8,200 additional checkouts. Reruns reuse the same two batches and ask Meilisearch to refresh its indexes.

All active fixture accounts use `ToscaTest2026!` on the local development database.

| Sign in | Email | What to test |
| --- | --- | --- |
| Customer | `buyer001@seed.tosca.invalid` | Order history, saved addresses, unavailable cart items, tracking and returns |
| Merchant owner | `owner01@seed.tosca.invalid` | Product pagination, sales charts, order fulfillment and team management |
| Merchant employee | `staff01-1@seed.tosca.invalid` | Pagi Studio orders and product actions |
| Admin | `admin@seed.tosca.invalid` | Moderation, manual payments, user and shop management, and marketplace charts |
| Suspended customer | `buyer240@seed.tosca.invalid` | Blocked customer sign-in |
| Suspended shop owner | `owner24@seed.tosca.invalid` | Blocked merchant sign-in |

## Run the search stress test

The Bun harness targets the local backend at `http://127.0.0.1:8080/api/search` by default and sends only `GET /api/search`. To use a separate seeded release backend for benchmarking, configure it from `backend/` in PowerShell.

```powershell
$env:DATABASE_URL = 'mysql://root:root@127.0.0.1:3306/tosca_market_stress'
$env:MEILI_INDEX_SUFFIX = '_stress'
$env:BIND = '127.0.0.1:8086'
$env:DEMO_CATALOG = 'false'
$env:ADMIN_EMAIL = ''
$env:ADMIN_PASSWORD = ''
cargo run --release --locked -- --seed-test
cargo run --release --locked
```

Edit `scripts/.env` to set the target URL, request rates, stage duration, pause between stages, search terms, and output directory. If you start the separate backend above, set `TOSCA_STRESS_URL=http://127.0.0.1:8086/api/search`. Leave `TOSCA_STRESS_SEARCH_BASELINE_URL` blank to test a single backend. For a comparison, set it to a second backend running against the same seeded data. The script alternates which backend goes first at each rate.

Run from the project root.

```powershell
bun scripts/stress-test.ts
```

The HTML report, JSON summary, and raw CSV measurements are saved in a new timestamped directory under `reports/` unless `TOSCA_STRESS_OUT` names another new directory. Each stage reports p50, p95, p99, successful requests per second, failures, and unsent requests.

### Search-only stress test with Python

With the benchmark backend running, run this from the project root. The script uses the Python 3.10+ standard library and sends only `GET /api/search` requests.

```powershell
python scripts/stress-search.py --url http://127.0.0.1:8086 --rps 10 --seconds 30
```

Each run uses one fixed request rate and writes an HTML report, JSON summary, and per-request CSV to a new directory under `reports/`. Pass `--out` to name the directory. Repeat `--query` to replace the six default seeded terms. The report includes p50, p95, p99, successful requests per second, HTTP failures, timeouts, and requests the driver could not send. The script exits with status 1 if p95 exceeds 250 ms or more than 1% of scheduled requests fail or go unsent.

To compare builds, run the same command against each backend with the same seed, query terms, rate, and duration. Run one build at a time and give the search service time to recover between tests that cause timeouts.

## Workspaces

- Customer sign-in and registration live at `/login` and `/register`. Guest orders are accessible from the browser that placed them while its guest session is valid.
- Merchant sign-in and registration live at `/merchant/login` and `/merchant/register`. Existing customers can open a shop at `/merchant`. In the merchant workspace, owners use **Team** to create employee accounts, deactivate or reactivate shop access, and reset passwords for accounts dedicated to their shop. A password reset revokes the employee's existing sessions.
- Admin sign-in lives at `/admin/login`. Admins approve taxonomy and products, manage users and shops, moderate reviews, configure couriers, and confirm manual payments under **Manual payments**.
- Shipping is Rp 1,000 per kg, calculated separately for each shop. Tax defaults to zero for development and can be configured in basis points. Merchants enter tracking details manually. QRIS and bank logos are disabled placeholders for future gateway integration.
- For password recovery, configure `SMTP_URL`, `SMTP_FROM`, and `APP_URL`. Each search runs two concurrent Meilisearch requests, one per index. Both match by name. The API returns up to six products (`items`) with `id`, `name`, and `lowest_price`, and up to four shops (`shops`) with `id` and `name`. Full results and autocomplete show the groups separately, with products first. The index definitions live in `backend/src/search/products.rs` and `backend/src/search/shops.rs`. The worker applies the settings to existing indexes and syncs committed catalog changes from MariaDB through a durable queue. Search visibility updates asynchronously and the API returns HTTP 503 while Meilisearch is unavailable.

## Search indexes

To use a new `MEILI_INDEX_SUFFIX`, stop the backend that serves that database, update `backend/.env`, and run `cargo run` from `backend/`. Startup creates missing indexes, applies the settings defined in `backend/src/search/products.rs` and `backend/src/search/shops.rs`, then backfills active products and nonsuspended shops from MariaDB. It waits for Meilisearch's tasks before opening the API port. If initialization fails, the startup error identifies the search setup step and pending changes stay queued for the next run.

The background worker continues syncing catalog changes and recreates a missing index while the backend is running. Give each database a distinct suffix if they share a Meilisearch server. Use the same suffix for all running backends that share one database and its sync queue.

Run `cargo check` from `backend/` and `bun run check` from `frontend/` for development validation.

Demo photos are from [Unsplash](https://unsplash.com/). Checkout assets come from Wikimedia Commons. [QRIS](https://commons.wikimedia.org/wiki/File:Logo_QRIS.svg), [BCA](https://commons.wikimedia.org/wiki/File:Bank_Central_Asia.svg), [BRI](https://commons.wikimedia.org/wiki/File:BRI_2025.svg), [Mandiri](https://commons.wikimedia.org/wiki/File:Bank_Mandiri_logo_2016.svg).
