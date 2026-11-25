# Tosca Market

A compact Indonesian marketplace with a Svelte storefront and separate merchant and admin workspaces.

| Folder      | Contents                                                                               |
| ----------- | -------------------------------------------------------------------------------------- |
| `frontend/` | Svelte 5.58.0, Bun 1.4.4, Vite, and UI assets                                          |
| `backend/`  | Rust 1.100.0, Axum 0.8.9, MariaDB migrations, demo fixtures, and backend configuration |

## Run locally

### 1. Install the prerequisites

| Dependency         | Download and setup                                                                                                                                                                                                                                      |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Git                | Install Git if you need to clone the repository. [Git downloads](https://git-scm.com/downloads/)                                                                                                                                                        |
| Rust 1.100.0       | Install with `rustup`. The backend pins the toolchain in [`rust-toolchain.toml`](backend/rust-toolchain.toml). On Windows, install the Visual Studio C++ Build Tools if prompted by the installer. [Install Rust](https://rust-lang.org/tools/install/) |
| Bun 1.4.4          | Use Bun to install frontend dependencies and run Vite. [Bun downloads](https://bun.com/), [v1.4.4 release](https://github.com/oven-sh/bun/releases/tag/bun-v1.4.4)                                                                                      |
| MariaDB 12.3.4     | Choose the server package for your platform. On Windows, the MSI can install MariaDB as a service. Set a root password and enable TCP/IP on port `3306` during setup. [Download MariaDB](https://mariadb.org/download/?t=mariadb&p=mariadb&r=12.3.4)    |
| Meilisearch 1.57.0 | Download the binary for your OS and architecture from the release assets. For Windows x64, use `meilisearch-windows-amd64.exe`. [Meilisearch release](https://github.com/meilisearch/meilisearch/releases#release-v1.57.0)                              |

To clone the repository and enter the project root, run the following commands.

```powershell
git clone https://github.com/khadafisani/ecommerce-experiment.git
cd ecommerce-experiment
```

### 2. Start MariaDB and Meilisearch

Start the MariaDB service and make sure it accepts connections on `127.0.0.1:3306`. In a separate terminal, run Meilisearch from the folder containing the downloaded binary. This PowerShell example binds it to the loopback address.

```powershell
.\meilisearch-windows-amd64.exe --env development --http-addr 127.0.0.1:7700
```

Keep Meilisearch running and check [its health endpoint](http://127.0.0.1:7700/health) for `{"status":"available"}`.

### 3. Configure and start the backend

From the project root, create `backend/.env` from the example if you have not configured it yet.

```powershell
cd backend
Copy-Item .env.example .env
```

Edit `backend/.env` before starting the API.

- Set `DATABASE_URL` to your MariaDB connection URL, for example `mysql://root:YOUR_PASSWORD@127.0.0.1:3306/tosca_market`. Percent-encode reserved characters in the password. The database user needs permission to create the named database if it is missing and to apply migrations. Remote MariaDB connections require `ssl-mode=VERIFY_IDENTITY` and a trusted server certificate.
- Set `JWT_SECRET` to a randomly generated value of at least 32 bytes. Run `[guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')` in PowerShell and paste the result into `.env`.
- Set `APP_URL` to the exact storefront origin, including any port. Outside loopback development it must use HTTPS. Browser writes are checked against this origin and JWTs use HttpOnly cookies.
- Keep `MEILI_URL=http://127.0.0.1:7700` for the loopback setup above. Leave `MEILI_KEY` blank when Meilisearch runs without authentication. For a keyed instance, use a key that can manage indexes and search them.
- To seed an administrator, set both `ADMIN_EMAIL` and `ADMIN_PASSWORD`. For a sample storefront, set `DEMO_CATALOG=true`. Set `MANUAL_PAYMENT_INSTRUCTIONS` if you want custom instructions shown to buyers after ordering.

Run `cargo run` from `backend/`. Startup creates the database if needed, applies migrations, and prepares the search indexes before the API starts listening on `127.0.0.1:8080`. Wait for the `Tosca Market ready` log message.

### 4. Start the frontend

In a new terminal at the project root, run the following commands.

```powershell
cd frontend
bun install --frozen-lockfile
bun run dev --host 127.0.0.1
```

Open the [development storefront](http://127.0.0.1:5173). Vite forwards API and upload requests to port `8080`.

To serve a built storefront through Axum, run `bun run build` from `frontend/`, set `APP_URL=http://127.0.0.1:8080` in `backend/.env`, restart the API, and open [the backend URL](http://127.0.0.1:8080).

## Seed the local test marketplace

With `backend/.env` configured for your local database, run this from `backend/`.

```powershell
cargo run -- --seed-test
```

If the API is already running on Windows, use `cargo run --target-dir target/seed-tool -- --seed-test` to build the seed command separately.

The seed adds 24 shops, 768 products, 2,304 variants, 240 buyers, 48 shop employees, and 10,000 checkouts. Purchase dates span about four years, with more activity in recent months. Seeded listings display product initials in place of fixture photos. The original checkout scenarios, tracking, reviews, returns, notifications, and moderation queues are included. An existing 1,800-checkout seed receives 8,200 additional checkouts. Reruns reuse the same two batches and ask Meilisearch to refresh its indexes.

Set `ALLOW_TEST_ACCOUNTS=true` only on the isolated local development backend to sign in to fixture accounts with `ToscaTest2026!`. Fixture sign-in and existing fixture sessions are disabled by default.

| Sign in              | Email                          | What to test                                                                  |
| -------------------- | ------------------------------ | ----------------------------------------------------------------------------- |
| Customer             | `buyer001@seed.tosca.invalid`  | Order history, saved addresses, unavailable cart items, tracking and returns  |
| Merchant owner       | `owner01@seed.tosca.invalid`   | Product pagination, sales charts, order fulfillment and team management       |
| Merchant employee    | `staff01-1@seed.tosca.invalid` | Pagi Studio orders and product actions                                        |
| Admin                | `admin@seed.tosca.invalid`     | Moderation, manual payments, user and shop management, and marketplace charts |
| Suspended customer   | `buyer240@seed.tosca.invalid`  | Blocked customer sign-in                                                      |
| Suspended shop owner | `owner24@seed.tosca.invalid`   | Blocked merchant sign-in                                                      |

## Workspaces

- Customer sign-in and registration live at `/login` and `/register`. Guest orders are accessible from the browser that placed them while its guest session is valid.
- When updating from the previous browser-storage session format, registered users sign in again. A valid existing guest session can move to the cookie format on the same browser.
- Merchant sign-in and registration live at `/merchant/login` and `/merchant/register`. Existing customers can open a shop at `/merchant`. In the merchant workspace, owners use **Team** to create employee accounts, deactivate or reactivate shop access, and reset passwords for accounts dedicated to their shop. A password reset revokes the employee's existing sessions.
- Admin sign-in lives at `/admin/login`. Admins approve taxonomy and products, manage users and shops, moderate reviews, configure couriers, and confirm manual payments under **Manual payments**.
- Unpaid checkouts reserve stock for 24 hours by default. Set `PAYMENT_HOLD_HOURS` from 1 to 720 to match your manual payment window. Expired unpaid checkouts are cancelled and their stock is returned automatically.
- Each upload is limited to 10 MiB. Before upload, the browser resizes images larger than that to at most 1,440 pixels on their longest side. The server still generates its responsive image sizes. MP4 and WebM files must already fit within 10 MiB.
- Shipping is Rp 1,000 per kg, calculated separately for each shop. Tax defaults to zero for development and can be configured in basis points. Merchants enter tracking details manually. QRIS and bank logos are disabled placeholders for future gateway integration.
- For password recovery, configure `SMTP_URL`, `SMTP_FROM`, and `APP_URL`. Each search runs two concurrent Meilisearch requests, one per index. Products match by name, brand, and category. Shops match by name. The API returns up to six products (`items`) with `id`, `name`, and `lowest_price`, and up to four shops (`shops`) with `id` and `name`. Full results and autocomplete show the groups separately, with products first. The index definitions live in `backend/src/search/products.rs` and `backend/src/search/shops.rs`. The worker applies the settings to existing indexes and syncs committed catalog changes from MariaDB through a durable queue. Search visibility updates asynchronously and the API returns HTTP 503 while Meilisearch is unavailable.

## Recommendations and semantic search

- Set `RECOMMENDATIONS_ENABLED=true` to enable **For you** in Explore products and related recommendations on product pages. The recommendations API accepts an optional `product_id` context, `category_id`, `shop_id`, `min_price`, `max_price`, and `sort` (`relevance`, `newest`, `price_asc`, or `price_desc`). Recommendation results are private and excluded from HTTP caching.
- Each Axum process keeps a private Stoolap snapshot in its operating-system temporary directory. The snapshot includes up to 96 recent listings per category, up to 96 recent listings across the catalog, and up to 96 products from each of the sales and views leaderboards. Older products in a viewer's history can still inform ranking. The snapshot and rankings expire after 60 seconds. MariaDB checks current availability and price before returning cards and fills pages that have too few recommendations. Guest activity lives in the same browser for 30 days. Account view and cart signals are retained in MariaDB for 90 days.
- On the storefront home, Explore products defaults to **For you**, which lists shopper preferences first, then products trending over the past seven days, then the rest of the catalog newest first. Shoppers can switch to **Newest** for chronological browsing. Guest views contribute to aggregate product counts without sending a browser identifier. The feed uses a signed cursor so shoppers can continue scrolling across backend replicas.
- Set `SEMANTIC_SEARCH_ENABLED=true` to blend semantic and keyword results for products in autocomplete and submitted search. Product search uses keywords if the embedding model is unavailable or takes more than one second, including time spent waiting for the model. Shop search and explicit `suggest=true` requests use keywords. Either flag starts the local Multilingual E5 Small model and a worker that fills missing catalog embeddings. The Hugging Face cache can be prewarmed and specified with `HF_HOME`. Inspect startup logs for `local text embedder ready` and `product_embeddings.next_attempt_at` for pending backfill work.
- Leave Stoolap disabled by default while benchmarking. In a multi-pod deployment, use the same MariaDB URL, Meilisearch index suffix, and feature settings across replicas. Each pod builds its cache independently.

Run the latency comparison from `backend/` with a local seeded database. The comparison uses two independent Stoolap caches, reads the same precomputed product IDs from MariaDB, and includes the live MariaDB availability check in both timings.

```powershell
$env:RECOMMENDATIONS_BENCH_DATABASE_URL = 'mysql://root:root@127.0.0.1:3306/tosca_market'
cargo test --release compare_hydrated_ranking_latency -- --ignored --nocapture
```

## Search indexes

To use a new `MEILI_INDEX_SUFFIX`, stop the backend that serves that database, update `backend/.env`, and run `cargo run` from `backend/`. Startup creates missing indexes, applies the settings defined in `backend/src/search/products.rs` and `backend/src/search/shops.rs`, then backfills active products and nonsuspended shops from MariaDB. It waits for Meilisearch's tasks before opening the API port. If initialization fails, the startup error identifies the search setup step and pending changes stay queued for the next run.

The background worker continues syncing catalog changes and recreates a missing index while the backend is running. Give each database a distinct suffix if they share a Meilisearch server. Use the same suffix for all running backends that share one database and its sync queue.

Admin product searches use the existing storefront product index for words and phrases. MariaDB supplies moderation-only listings and shop-name matches. For broad queries the API tries a time-limited catalog-wide chronological search in MariaDB. If that does not fill the page in time, the API orders the indexed matches by date.

Run `cargo check` from `backend/` and `bun run check` from `frontend/` for development validation.

Demo photos are from [Unsplash](https://unsplash.com/). Checkout assets come from Wikimedia Commons. [QRIS](https://commons.wikimedia.org/wiki/File:Logo_QRIS.svg), [BCA](https://commons.wikimedia.org/wiki/File:Bank_Central_Asia.svg), [BRI](https://commons.wikimedia.org/wiki/File:BRI_2025.svg), [Mandiri](https://commons.wikimedia.org/wiki/File:Bank_Mandiri_logo_2016.svg).
