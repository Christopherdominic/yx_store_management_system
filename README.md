# XY Store — Multi-Store Retail Management System

A Rust backend for managing retail operations across multiple stores under a
single business: inventory, sales, stock transfers, customers, and expenses.

> **Status:** domain models cover the whole SRS. The HTTP API implements SRS
> Part A sections 1–2 — business and store management (FR-01 – FR-03) and
> accounts and access control (FR-04 – FR-06).

Built with [Axum](https://github.com/tokio-rs/axum), PostgreSQL, and
[SQLx](https://github.com/launchbadge/sqlx).

## Project structure

```
.
├── src/
│   ├── main.rs          # entry point: config, DB pool, migrations, server
│   ├── lib.rs           # library root; exposes `app()` for tests
│   ├── config.rs        # environment configuration
│   ├── state.rs         # shared application state
│   ├── models/          # domain data structures
│   ├── db/              # SQLx mappings for the models (IDs, enums, rows)
│   ├── routes/          # route table
│   ├── controllers/     # HTTP handlers: extract, authorize, respond
│   ├── services/        # business logic, SQL, access control, audit
│   ├── middlewares/     # bearer-token authentication extractor
│   ├── errors/          # AppError -> JSON error responses
│   └── utils/           # hashing, validation, extractors
├── migrations/          # SQL migrations (run automatically on start)
├── tests/               # end-to-end API tests
├── docs/                # requirements and design documents
└── docker-compose.yml   # local PostgreSQL
```

## Domain models

Defined under `src/models/`, one module per area:

| Module | Models | SRS |
| --- | --- | --- |
| `business` | `Business` | FR-01, FR-03 |
| `store` | `Store` (`Active` / `Inactive`) | FR-02 |
| `settings` | `BusinessSettings`, `StoreSettings` | FR-03 |
| `user` | `User`, `RoleAssignment`, `Role` | FR-04, FR-05 |
| `product`, `category`, `product_variant`, `price_history` | `Product`, `ProductCategory`, `ProductVariant`, `PriceHistory` ([details](docs/product-models.md)) | FR-07 – FR-09 |
| `inventory` | `InventoryItem`, `InventoryBatch`, `StockMovement` | FR-10 – FR-14 |
| `adjustment` | `StockCount`, `StockAdjustment` | FR-12 |
| `transfer` | `StockTransfer`, `StockTransferItem` | FR-22 |
| `sale` | `Sale`, `SaleItem`, `Payment` | FR-15 – FR-17 |
| `sale_return` | `SaleReturn`, `SaleReturnItem`, `Refund` | FR-18 |
| `customer` | `Customer` | FR-19 |
| `supplier` | `Supplier`, `SupplierProduct`, `SupplierLedgerEntry` | FR-20, FR-23 |
| `purchase` | `PurchaseOrder`, `GoodsReceipt`, `PurchaseReturn` (+ items) | FR-21, FR-23 |
| `expense` | `ExpenseCategory`, `Expense` | FR-24 |
| `notification` | `Notification` | FR-13, FR-26 |
| `audit` | `AuditLog` | FR-27 |
| `ids`, `common` | Typed IDs, `Money`, `Quantity`, `CurrencyCode`, `RecordRef`, `Address` | — |

Conventions:

- **Typed IDs** — every entity has its own UUID v7 ID type (`StoreId`,
  `ProductId`, ...), so IDs can't be mixed up.
- **Exact money** — amounts use `rust_decimal::Decimal`, never `f64`
  (NFR-30), and serialize to JSON as strings (`"1999.99"`).
- **UTC timestamps** — `created_at` / `updated_at` on every record.
- **No hard deletes** — records referenced by history are archived or
  deactivated instead.
- **Snapshots** — sales copy product name, SKU and unit price so receipts
  never change after a price update.

Models stay free of database code; `src/db` maps them to PostgreSQL (typed
IDs as `UUID`, enums as their snake_case name in `TEXT`, addresses as
`JSONB`).

## Getting started

Requires a recent [Rust toolchain](https://www.rust-lang.org/tools/install)
and Docker (for PostgreSQL).

```bash
docker compose up -d          # PostgreSQL on localhost:5432
cp .env.example .env          # adjust as needed
cargo run                     # applies migrations, listens on :8080
```

Run the tests (API tests each get their own temporary database):

```bash
DATABASE_URL=postgres://xy_store:xy_store@localhost:5432/xy_store cargo test
```

## Using the API

All endpoints are under `/api/v1`. Protected endpoints take
`Authorization: Bearer <token>` from `POST /auth/login`. Errors are JSON:
`{"error": {"code": "...", "message": "...", "details": [...]}}`.

```bash
# 1. Register a business together with its owner account
curl -X POST localhost:8080/api/v1/auth/register -H 'content-type: application/json' -d '{
  "business": {
    "legal_name": "Amiable Retail Ltd",
    "trading_name": "Amiable Retail",
    "address": { "line1": "12 Broad St", "city": "Lagos", "country": "NG" }
  },
  "owner": { "full_name": "Amaka Obi", "email": "amaka@amiable.ng", "password": "Passw0rd!" }
}'

# 2. Log in, then create a store
TOKEN=$(curl -s -X POST localhost:8080/api/v1/auth/login -H 'content-type: application/json' \
  -d '{"email":"amaka@amiable.ng","password":"Passw0rd!"}' | jq -r .token)
curl -X POST localhost:8080/api/v1/businesses/$BUSINESS_ID/stores -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"code":"LAG","name":"Lagos","address":{"line1":"Lagos Island","city":"Lagos","country":"NG"}}'
```

### Endpoints

| Method | Path | Permission | FR |
| --- | --- | --- | --- |
| POST | `/auth/register` | public (new business + owner) | FR-01, FR-04 |
| POST | `/auth/login` | public | FR-04 |
| POST | `/auth/logout` | authenticated | FR-04 |
| POST | `/auth/logout-all` | authenticated | FR-04 |
| GET | `/auth/me` | authenticated | FR-04 |
| POST | `/auth/password/change` | authenticated | FR-04 |
| POST | `/auth/password/forgot` | public | FR-04 |
| POST | `/auth/password/reset` | public (reset token) | FR-04 |
| GET | `/roles` | public | FR-05 |
| GET | `/businesses/{business_id}` | `business_view` | FR-01 |
| PATCH | `/businesses/{business_id}` | `business_update` | FR-01 |
| GET | `/businesses/{business_id}/settings` | `business_view` | FR-03 |
| PATCH | `/businesses/{business_id}/settings` | `business_settings_update` | FR-03 |
| GET | `/businesses/{business_id}/stores?q=&status=` | `store_view` | FR-02 |
| POST | `/businesses/{business_id}/stores` | `store_create` | FR-02 |
| GET | `/businesses/{business_id}/stores/{store_id}` | `store_view` | FR-02 |
| PATCH | `/businesses/{business_id}/stores/{store_id}` | `store_update` | FR-02 |
| POST | `/businesses/{business_id}/stores/{store_id}/activate` | `store_status_change` | FR-02 |
| POST | `/businesses/{business_id}/stores/{store_id}/deactivate` | `store_status_change` | FR-02 |
| GET | `/businesses/{business_id}/stores/{store_id}/settings` | `store_view` | FR-03 |
| PATCH | `/businesses/{business_id}/stores/{store_id}/settings` | `store_settings_update` | FR-03 |
| GET | `/businesses/{business_id}/employees?q=&store_id=&status=` | `employee_view` | FR-05 |
| POST | `/businesses/{business_id}/employees` | `employee_manage` | FR-05 |
| GET | `/businesses/{business_id}/employees/{user_id}` | `employee_view` | FR-05 |
| PATCH | `/businesses/{business_id}/employees/{user_id}` | `employee_manage` | FR-05 |
| POST | `/businesses/{business_id}/employees/{user_id}/deactivate` | `employee_manage` | FR-05 |
| POST | `/businesses/{business_id}/employees/{user_id}/reactivate` | `employee_manage` | FR-05 |
| PUT | `/businesses/{business_id}/employees/{user_id}/business-role` | `business_role_manage` | FR-05 |
| PUT, DELETE | `/businesses/{business_id}/employees/{user_id}/stores/{store_id}` | `employee_manage` | FR-05 |
| GET | `/businesses/{business_id}/audit-logs` | `audit_log_view` | FR-01, FR-06 |

PATCH bodies are partial: omitted fields are unchanged and `""` clears an
optional field. Audit logs filter by `actor_user_id`, `action`, `store_id`,
`record_id`, `outcome`, `from`, `to`, `limit`, and `offset`.

### Roles

| Role | Scope | Can |
| --- | --- | --- |
| `business_owner` | all stores | everything, including granting owner/administrator |
| `administrator` | all stores | everything except granting business roles or modifying owners/administrators |
| `store_manager` | assigned store | view and edit the store and its settings, view its staff |
| `cashier`, `inventory_officer`, `finance_officer` | assigned store | view the store |

Each user belongs to one business. They hold at most one business-wide role
and one role per store, so they can be a manager in Lagos and a cashier in
Abuja. Changing a role revokes the old `RoleAssignment` and adds a new one,
keeping the history. Permissions are computed per request from the database
for the business/store in the URL; a store the user is not assigned to
returns `403` and is written to the audit log as `access_denied`.

### Rules worth knowing

- **Sessions** are opaque random tokens; only their SHA-256 is stored. They
  expire after `SESSION_TTL_MINUTES`, or after `SESSION_IDLE_MINUTES` without
  use, and are revoked on logout, password change (other sessions), password
  reset, and employee deactivation.
- **Passwords** are hashed with Argon2id and kept in `user_credentials`,
  apart from `users`. They need 8–128 characters with a letter and a digit;
  five consecutive failures lock the account for `LOCKOUT_MINUTES`.
- **Password reset** tokens are single-use and expire after
  `PASSWORD_RESET_TTL_MINUTES`. No email provider is wired up yet: set
  `EXPOSE_RESET_TOKENS=true` in development to receive the token in the
  response.
- **Settings effective-date policy (FR-03):** settings changes take effect
  immediately for transactions created afterwards. Transactions store the
  currency, tax, and price values they were created with, so changing
  settings never rewrites history. Every change is audited with before/after
  values.
- **Stores** are never deleted. A deactivated store keeps its history; later
  modules must call `services::stores::ensure_operational` before recording
  sales, stock movements, purchases, or transfers.
- **Audit logs** are append-only; a database trigger rejects updates and
  deletes.

## Not yet covered

- MFA for administrative functions (NFR-17) and API rate limiting (NFR-20).
- Email delivery for password reset.
- Endpoints for FR-07 onwards (products, inventory, sales, ...).

## Documentation

See [`docs/Store-Management-System-SRS-v1.0.pdf`](docs/Store-Management-System-SRS-v1.0.pdf)
for the full requirements specification.
