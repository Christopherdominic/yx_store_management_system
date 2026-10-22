# Product management models

The models cover FR-07, FR-08, and FR-09 in the SRS. They live in
`src/models/` and are exported from `xy_store::models`. IDs are typed UUIDs (`ProductId`, `ProductCategoryId`, ...) and prices use
the shared `Money` type, consistent with the other domain models.
Relationships use IDs rather than nested copies of entities.

- `Product`: business-owned catalogue entry, category, identifiers (SKU,
  barcode), descriptive fields (brand, image, extra attributes), unit of
  measurement, cost/selling prices, lifecycle, and timestamps.
- `ProductCategory`: business-owned category with an active/archived lifecycle.
  Categories can be nested through an optional parent.
- `ProductVariant`: independently identified, priced, and stocked variation of a
  parent product. Attributes hold values such as size and colour; variants use
  the parent's unit of measurement.
- `PriceHistory`: one cost or selling price change for a product or variant,
  including old/new values, effective time, actor, approver where required,
  recording time, and reason.

Prices use `Decimal` and serialize as strings to preserve decimal precision.
They are expressed in the business currency; currency configuration and allowed
precision must be enforced by the application. All timestamps use UTC.

## How the API enforces the rules

The endpoints live in `src/services/products.rs` and
`src/services/categories.rs` (see the README for the list).

- **Validation:** names are required, SKUs are 1-64 characters of A-Z, 0-9,
  `-`, `_` or `.` (uppercased), barcodes are letters, digits or `-`, and
  prices are not negative with at most 2 decimal places.
- **Same business:** every query is scoped to the business in the URL, so a
  category, product or variant from another business is reported as not
  found.
- **Unique codes:** SKUs and barcodes are registered in the `catalog_codes`
  table in the same transaction as their product or variant. Its unique
  indexes reject duplicates across products and variants, even when two
  requests race.
- **No deletes:** categories, products and variants are archived and can be
  restored. Archived products can't get new variants or price changes;
  archived categories can't get new products or subcategories. A category
  can't be moved under itself or one of its subcategories.
- **Prices:** prices change only through `PATCH .../prices`. The new price
  and one history entry per changed price are written in one transaction,
  with the user and an optional reason. History rows can't be updated or
  deleted (a database trigger blocks it). Every change is also in the audit
  log as `price_changed`.

Not done yet: approval thresholds for price changes (FR-29) and
future-dated price changes. Changes take effect immediately, and
`approved_by` is always empty for now.

Inventory, sale items, and stock movements already carry an optional variant
ID, and completed sale items keep their own unit-price and discount snapshots.

## Files

| File | Models |
| --- | --- |
| `src/models/product.rs` | `Product`, `UnitOfMeasure` |
| `src/models/category.rs` | `ProductCategory` |
| `src/models/product_variant.rs` | `ProductVariant` |
| `src/models/price_history.rs` | `PriceHistory`, `PriceType` |
| `src/services/categories.rs`, `src/services/products.rs` | Business rules and SQL |
| `src/controllers/categories.rs`, `src/controllers/products.rs` | HTTP handlers |
| `migrations/0002_products.sql` | Tables |
| `tests/products.rs` | API tests |

Run the tests with `DATABASE_URL=... cargo test` (see the README).
