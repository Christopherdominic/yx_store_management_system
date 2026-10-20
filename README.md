# XY Store

A backend for retail businesses that run several stores. One business, such
as Amiable Retail Ltd with stores in Lagos, Abuja and Kaduna, manages its
stores, staff, products, stock, sales, purchasing and expenses from one
system, with each employee seeing only the stores they work in.

Built in Rust with [Axum](https://github.com/tokio-rs/axum),
[SQLx](https://github.com/launchbadge/sqlx) and PostgreSQL. Requirements are
defined in the [SRS](docs/Store-Management-System-SRS-v1.0.pdf); requirement
IDs such as FR-07 below refer to it.

## Status

| Area | SRS | Domain models | API |
| --- | --- | :---: | :---: |
| Business and store management | FR-01 – FR-03 | ✅ | ✅ |
| Accounts and access control | FR-04 – FR-06 | ✅ | ✅ |
| Product management | FR-07 – FR-09 | ✅ | — |
| Inventory and stock control | FR-10 – FR-14 | ✅ | — |
| Sales, returns and customers | FR-15 – FR-19 | ✅ | — |
| Purchasing, suppliers and transfers | FR-20 – FR-23 | ✅ | — |
| Expenses, notifications and audit | FR-24 – FR-27 | ✅ | Audit log only |
| Reporting, import/export, approval rules, admin | FR-25, FR-28 – FR-30 | — | — |

## Quick start

You need a recent [Rust toolchain](https://www.rust-lang.org/tools/install)
and Docker.

```bash
docker compose up -d     # PostgreSQL on localhost:5432
cp .env.example .env     # defaults work for local development
cargo run                # applies migrations, serves on http://localhost:8080
```

Check it is running with `curl localhost:8080/health`.

### Running the tests

```bash
DATABASE_URL=postgres://xy_store:xy_store@localhost:5432/xy_store cargo test
```

Unit tests need no database. The API tests in `tests/api.rs` need
`DATABASE_URL`; each one runs against its own temporary database, so they
never touch your development data.

Before opening a pull request, also run `cargo fmt` and
`cargo clippy --all-targets`.

### Configuration

Settings are read from environment variables (or `.env`):

| Variable | Default | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | — (required) | PostgreSQL connection string |
| `BIND_ADDR` | `0.0.0.0:8080` | Address the server listens on |
| `DB_MAX_CONNECTIONS` | `10` | Size of the database connection pool |
| `RUST_LOG` | `xy_store=info,tower_http=info` | Log filter |
| `SESSION_TTL_MINUTES` | `720` | Maximum session lifetime |
| `SESSION_IDLE_MINUTES` | `15` | Session expires after this long unused |
| `MAX_FAILED_LOGINS` | `5` | Failed logins before the account locks |
| `LOCKOUT_MINUTES` | `15` | How long a locked account stays locked |
| `PASSWORD_RESET_TTL_MINUTES` | `30` | Lifetime of a password reset token |
| `ALLOW_OWNER_SIGNUP` | `true` | Allow public registration of new businesses |
| `EXPOSE_RESET_TOKENS` | `false` | **Development only.** Return reset tokens in the API response, since no email provider is set up yet |

## How the code is organised

```
src/
├── main.rs          # starts the server: config, database pool, migrations
├── lib.rs           # crate root; exposes app() so tests can build the router
├── config.rs        # environment configuration
├── state.rs         # shared state passed to every handler
├── routes/          # the route table
├── middlewares/     # bearer-token authentication
├── controllers/     # HTTP handlers: parse the request, check permission, respond
├── services/        # business rules, SQL, access control, audit logging
├── db/              # how models are stored in PostgreSQL
├── models/          # domain types, free of HTTP and database code
├── errors/          # AppError and its JSON error responses
└── utils/           # password hashing, validation, extractors
migrations/          # SQL migrations, applied automatically on start
tests/               # end-to-end API tests
docs/                # requirements and design notes
```

A request flows through these layers in order:

```
route → middleware (who is calling?) → controller (may they do this?)
      → service (do it, write the audit log) → database
```

Keep each layer to its job: controllers don't run SQL, services don't know
about HTTP, and models don't depend on either.

### Adding a feature

To build the API for a new area, for example products (FR-07 – FR-09):

1. **Migration.** Add `migrations/000N_<name>.sql` with the tables.
2. **Models.** The domain types already exist in `src/models/`. Adjust them
   if needed.
3. **Database mapping.** Teach SQLx to read and write the models in `src/db/`.
4. **Service.** Add `src/services/<name>.rs` with the business rules. Check
   permissions there, call `services::stores::ensure_operational` before
   changing anything in a store, and write audit entries for every change.
5. **Controller and routes.** Add handlers in `src/controllers/<name>.rs` and
   register them in `src/routes/mod.rs`.
6. **Tests.** Add API tests with `#[sqlx::test]` in `tests/`.

Work on a branch, open a pull request against `main`, and merge `main` into
your branch if it falls behind.

## Domain models

Every area of the SRS has models in `src/models/`, one module per area:

| Module | Models | SRS |
| --- | --- | --- |
| `business`, `store`, `settings` | `Business`, `Store`, `BusinessSettings`, `StoreSettings` | FR-01 – FR-03 |
| `user`, `employee`, `permission` | `User`, `RoleAssignment`, `Role`, `Employee`, `Permission` | FR-04 – FR-06 |
| `product`, `category`, `product_variant`, `price_history` | `Product`, `ProductCategory`, `ProductVariant`, `PriceHistory` ([details](docs/product-models.md)) | FR-07 – FR-09 |
| `inventory` | `InventoryItem`, `InventoryBatch`, `StockMovement` | FR-10 – FR-14 |
| `adjustment` | `StockCount`, `StockAdjustment` | FR-12 |
| `sale` | `Sale`, `SaleItem`, `Payment` | FR-15 – FR-17 |
| `sale_return` | `SaleReturn`, `SaleReturnItem`, `Refund` | FR-18 |
| `customer` | `Customer` | FR-19 |
| `supplier` | `Supplier`, `SupplierProduct`, `SupplierLedgerEntry` | FR-20, FR-23 |
| `purchase` | `PurchaseOrder`, `GoodsReceipt`, `PurchaseReturn` and their items | FR-21, FR-23 |
| `transfer` | `StockTransfer`, `StockTransferItem` | FR-22 |
| `expense` | `ExpenseCategory`, `Expense` | FR-24 |
| `notification` | `Notification` | FR-13, FR-26 |
| `audit` | `AuditLog` | FR-27 |
| `ids`, `common` | Typed IDs, `Money`, `Quantity`, `CurrencyCode`, `RecordRef`, `Address` | — |

They follow the same conventions:

- **Typed IDs.** Each entity has its own ID type (`StoreId`, `ProductId`, …)
  wrapping a UUID v7, so one kind of ID can't be passed where another is
  expected.
- **Exact money.** Amounts use `Decimal`, never floating point (NFR-30), and
  appear in JSON as strings such as `"1999.99"`.
- **UTC timestamps.** Records carry `created_at` and `updated_at`.
- **No hard deletes.** Records that history depends on are archived or
  deactivated instead.
- **Snapshots.** Sales keep the product name, SKU and price they were made
  with, so receipts never change when a price does.

In PostgreSQL, typed IDs are stored as `UUID`, enums as their snake_case name
in `TEXT`, and addresses as `JSONB`.

## Using the API

All endpoints are under `/api/v1`. Log in with `POST /auth/login` and send the
returned token as `Authorization: Bearer <token>`. Errors are JSON:
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

PATCH requests are partial: omitted fields stay unchanged and `""` clears an
optional field. Audit logs can be filtered by `actor_user_id`, `action`,
`store_id`, `record_id`, `outcome`, `from`, `to`, `limit` and `offset`.

### Roles and permissions

| Role | Scope | Can |
| --- | --- | --- |
| `business_owner` | All stores | Everything, including granting owner and administrator roles |
| `administrator` | All stores | Everything except granting business roles or changing owners and administrators |
| `store_manager` | Assigned store | View and edit the store and its settings, view its staff |
| `cashier`, `inventory_officer`, `finance_officer` | Assigned store | View the store |

Each user belongs to one business. They hold at most one business-wide role
and one role per store, so someone can be a manager in Lagos and a cashier in
Abuja. Changing a role revokes the old assignment and adds a new one, so the
history is kept.

Permissions are checked on every request against the database, for the
business and store in the URL. Asking for a store you aren't assigned to
returns `403` and is recorded in the audit log as `access_denied`.

### Security and data rules

- **Sessions** are random tokens; only their SHA-256 hash is stored. They end
  after `SESSION_TTL_MINUTES`, after `SESSION_IDLE_MINUTES` without use, or on
  logout. Changing a password signs out your other sessions; a password reset
  or deactivating an employee signs out all of theirs.
- **Passwords** are hashed with Argon2id and stored apart from user records.
  They need 8–128 characters including a letter and a digit. After
  `MAX_FAILED_LOGINS` failures in a row the account locks for
  `LOCKOUT_MINUTES`.
- **Password reset tokens** work once and expire after
  `PASSWORD_RESET_TTL_MINUTES`.
- **Settings changes** (FR-03) apply to transactions created afterwards.
  Transactions keep the currency, tax and prices they were made with, so a
  settings change never rewrites history. Every change is audited with its
  before and after values.
- **Stores** are never deleted. A deactivated store keeps its history but
  accepts no new activity.
- **Audit logs** can't be edited or deleted; a database trigger blocks it.

## Not yet covered

- API endpoints for FR-07 onwards (products, inventory, sales, purchasing,
  expenses, notifications).
- Reporting, CSV import/export, configurable approval rules and system
  administration (FR-25, FR-28 – FR-30).
- Email delivery for password resets.
- Multi-factor authentication for administrators (NFR-17) and API rate
  limiting (NFR-20).

## Documentation

- [Software Requirements Specification](docs/Store-Management-System-SRS-v1.0.pdf):
  the functional (FR) and non-functional (NFR) requirements.
- [Product models](docs/product-models.md): design notes for FR-07 – FR-09.
