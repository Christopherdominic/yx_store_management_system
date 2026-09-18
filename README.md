# XY Store — Multi-Store Retail Management System

A Rust backend for managing retail operations across multiple stores under a
single business: inventory, sales, stock transfers, customers, and expenses.

> **Status:** early scaffolding. Domain models are drafted; the HTTP layer
> (controllers, routes, services, middlewares) is not yet implemented.

## Project structure

```
.
├── src/
│   ├── main.rs          # entry point
│   ├── models/          # domain data structures
│   ├── controllers/     # request handlers (planned)
│   ├── routes/          # route definitions (planned)
│   ├── services/        # business logic (planned)
│   ├── middlewares/     # request/response middleware (planned)
│   ├── errors/          # error types (planned)
│   └── utils/           # shared helpers (planned)
├── docs/                 # requirements and design documents
├── Cargo.toml
└── Cargo.lock
```

## Domain models

Defined under `src/models/`:

- **Business / Store** — a business with one or more stores, each with a
  status (`Active` / `Inactive`).
- **User / StoreEmployee** — users assigned to a store with a role
  (`Manager`, `Cashier`, `InventoryOfficer`).
- **Product** — catalog items belonging to a business (SKU, price).
- **Inventory** — per-store stock levels and reorder thresholds for a product.
- **StockTransfer** — movement of stock between stores, with a status
  (`Pending`, `Approved`, `Rejected`, `Completed`, `Cancelled`).
- **Customer** — customers attached to a business.
- **Sale / SaleItem** — point-of-sale transactions and their line items.
- **Expense** — store expenses categorized (`Rent`, `Salary`,
  `Electricity`, etc.).

## Getting started

Requires a recent [Rust toolchain](https://www.rust-lang.org/tools/install).

```bash
cargo build
cargo run
```

## Documentation

See [`docs/multi_store_retail_management_system_requirements.docx`](docs/multi_store_retail_management_system_requirements.docx)
for the full requirements specification.
