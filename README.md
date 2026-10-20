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
│   ├── lib.rs           # library root
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

Defined under `src/models/`, one module per area:

| Module | Models | SRS |
| --- | --- | --- |
| `business` | `Business` | FR-01, FR-03 |
| `store` | `Store` (`Active` / `Inactive`) | FR-02 |
| `user` | `User`, `RoleAssignment`, `Role` | FR-04, FR-05 |
| `product` | `ProductCategory`, `Product`, `ProductVariant`, `PriceChange` | FR-07 – FR-09 |
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

## Getting started

Requires a recent [Rust toolchain](https://www.rust-lang.org/tools/install).

```bash
cargo build
cargo run
```

## Documentation

See [`docs/multi_store_retail_management_system_requirements.docx`](docs/multi_store_retail_management_system_requirements.docx)
for the full requirements specification.
