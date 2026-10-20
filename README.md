# XY Store — Multi-Store Retail Management System

A Rust backend for managing retail operations across multiple stores under a
single business: inventory, sales, stock transfers, customers, and expenses.

> **Status:** SRS Part A sections 1–2 are implemented — business and store
> management (FR-01 – FR-03) and accounts and access control (FR-04 – FR-06).
> Later sections (products, inventory, sales, …) are still draft models.

Built with [Axum](https://github.com/tokio-rs/axum), PostgreSQL, and
[SQLx](https://github.com/launchbadge/sqlx).

## Project structure

```
.
├── src/
│   ├── main.rs          # entry point: config, DB pool, migrations, server
│   ├── lib.rs           # crate root; exposes `app()` for tests
│   ├── config.rs        # environment configuration
│   ├── state.rs         # shared application state
│   ├── routes/          # route table
│   ├── controllers/     # HTTP handlers: extract, authorize, respond
│   ├── services/        # business logic, SQL, access control, audit
│   ├── middlewares/     # bearer-token authentication extractor
│   ├── models/          # domain types and DB rows
│   ├── errors/          # AppError -> JSON error responses
│   └── utils/           # hashing, validation, extractors
├── migrations/          # SQL migrations (run automatically on start)
├── tests/               # end-to-end API tests
├── docs/                # requirements and design documents
└── docker-compose.yml   # local PostgreSQL
```

## Getting started

Requires a recent [Rust toolchain](https://www.rust-lang.org/tools/install)
and Docker (for PostgreSQL).

```bash
docker compose up -d          # PostgreSQL on localhost:5432
cp .env.example .env          # adjust as needed
cargo run                     # applies migrations, listens on :8080
```

Run the tests (each test gets its own temporary database):

```bash
DATABASE_URL=postgres://xy_store:xy_store@localhost:5432/xy_store cargo test
```

## Using the API

All endpoints are under `/api/v1`. Protected endpoints take
`Authorization: Bearer <token>` from `POST /auth/login`. Errors are JSON:
`{"error": {"code": "...", "message": "...", "details": [...]}}`.

```bash
# 1. Register a business owner and log in
curl -X POST localhost:8080/api/v1/auth/register -H 'content-type: application/json' \
  -d '{"full_name":"Amaka Obi","email":"amaka@amiable.ng","password":"Passw0rd!"}'
TOKEN=$(curl -s -X POST localhost:8080/api/v1/auth/login -H 'content-type: application/json' \
  -d '{"email":"amaka@amiable.ng","password":"Passw0rd!"}' | jq -r .token)

# 2. Create the business (the creator becomes its owner), then a store
curl -X POST localhost:8080/api/v1/businesses -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"legal_name":"Amiable Retail Ltd","trading_name":"Amiable Retail","address":"12 Broad St, Lagos"}'
curl -X POST localhost:8080/api/v1/businesses/$BUSINESS_ID/stores -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' -d '{"code":"LAG-01","name":"Lagos","address":"Lagos Island"}'
```

### Endpoints

| Method | Path | Permission | FR |
| --- | --- | --- | --- |
| POST | `/auth/register` | public (business owners only) | FR-04 |
| POST | `/auth/login` | public | FR-04 |
| POST | `/auth/logout` | authenticated | FR-04 |
| POST | `/auth/logout-all` | authenticated | FR-04 |
| GET | `/auth/me` | authenticated | FR-04 |
| POST | `/auth/password/change` | authenticated | FR-04 |
| POST | `/auth/password/forgot` | public | FR-04 |
| POST | `/auth/password/reset` | public (reset token) | FR-04 |
| GET | `/roles` | public | FR-05 |
| GET, POST | `/businesses` | authenticated | FR-01 |
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
| GET | `/businesses/{business_id}/employees?q=&store_id=&is_active=` | `employee_view` | FR-05 |
| POST | `/businesses/{business_id}/employees` | `employee_manage` | FR-05 |
| GET | `/businesses/{business_id}/employees/{user_id}` | `employee_view` | FR-05 |
| PATCH | `/businesses/{business_id}/employees/{user_id}` | `employee_manage` | FR-05 |
| POST | `/businesses/{business_id}/employees/{user_id}/deactivate` | `employee_manage` | FR-05 |
| POST | `/businesses/{business_id}/employees/{user_id}/reactivate` | `employee_manage` | FR-05 |
| PUT | `/businesses/{business_id}/employees/{user_id}/business-role` | `business_role_manage` | FR-05 |
| PUT, DELETE | `/businesses/{business_id}/employees/{user_id}/stores/{store_id}` | `employee_manage` | FR-05 |
| GET | `/businesses/{business_id}/audit-logs` | `audit_log_view` | FR-01, FR-06 |

PATCH bodies are partial: omitted fields are unchanged and `""` clears an
optional field.

### Roles

| Role | Scope | Can |
| --- | --- | --- |
| `business_owner` | all stores | everything, including granting owner/administrator |
| `administrator` | all stores | everything except granting business roles or modifying owners/administrators |
| `store_manager` | assigned store | view and edit the store and its settings, view its staff |
| `cashier`, `inventory_officer`, `finance_officer` | assigned store | view the store |

A user has at most one role per store and can hold different roles at
different stores (e.g. manager in Lagos, cashier in Abuja). Permissions are
computed per request from the database for the business/store in the URL;
a store ID the user is not assigned to returns `403` and is written to the
audit log as `access.denied`.

### Rules worth knowing

- **Sessions** are opaque random tokens; only their SHA-256 is stored. They
  expire after `SESSION_TTL_MINUTES`, or after `SESSION_IDLE_MINUTES` without
  use, and are revoked on logout, password change (other sessions), password
  reset, and employee deactivation.
- **Passwords** are hashed with Argon2id, need 8–128 characters with a letter
  and a digit, and five consecutive failures lock the account for
  `LOCKOUT_MINUTES`.
- **Password reset** tokens are single-use and expire after
  `PASSWORD_RESET_TTL_MINUTES`. No email provider is wired up yet: set
  `EXPOSE_RESET_TOKENS=true` in development to receive the token in the
  response.
- **Settings effective-date policy (FR-03):** settings changes take effect
  immediately for transactions created afterwards. Transactions will store
  the currency, tax, and price values they were created with, so changing
  settings never rewrites history. Every change is audited with before/after
  values.
- **Stores** are never deleted. A deactivated store keeps its history; later
  modules must call `services::stores::ensure_active` before recording sales,
  stock movements, purchases, or transfers.
- **Tax rates** are integer basis points (`750` = 7.50%) — no floating point.
- **Audit logs** are append-only; a database trigger rejects updates and
  deletes.

## Not yet covered

- MFA for administrative functions (NFR-17) and API rate limiting (NFR-20).
- Email delivery for password reset.
- The draft models in `src/models/` for products, inventory, sales,
  customers, transfers, and expenses are not compiled yet.

## Documentation

See [`docs/multi_store_retail_management_system_requirements.docx`](docs/multi_store_retail_management_system_requirements.docx)
for the full requirements specification.
