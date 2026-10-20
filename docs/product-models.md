# Product management models

The models cover FR-07, FR-08, and FR-09 in the SRS. They live in
`src/models/` and are exported from `xy_store::models`. IDs remain `u64`, consistent with existing domain records.
Relationships use IDs rather than nested copies of entities.

- `Product`: business-owned catalogue entry, category, identifiers (SKU,
  barcode), descriptive fields (brand, image, extra attributes), unit of
  measurement, cost/selling prices, lifecycle, and timestamps.
- `Category`: business-owned category with an active/archived lifecycle.
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

## Service and persistence responsibilities

These are data models, not request validation or database migrations. Required
text must be nonblank, prices nonnegative, and attribute names/values valid.
Category, product, variant, and actor relationships must belong to the same
business. A history variant must belong to its recorded parent product.

Enforce SKU uniqueness across products and variants within each business, and
uniqueness of assigned barcodes within the same lookup scope. Use database
constraints or a shared identifier registry to prevent concurrent duplicates.
Archive referenced entities instead of deleting them. Define category archival
policy and prevent new sales of archived products or variants in services.

Apply price changes with authorization and configured approval thresholds. Write
price history and current prices atomically; a change to both cost and selling
price produces two history entries. Future-effective changes require scheduling
in the service layer so they do not replace current prices prematurely.

Inventory, sale items, and stock movements will need an optional variant ID to
support independently stocked variants. Completed sale items must retain decimal
unit-price and discount snapshots. These integrations and the existing unrelated
model scaffolds are outside this product-model change; their existing `f64`
monetary fields still need migration before financial processing is implemented.

## Files

| File | Models |
| --- | --- |
| `src/models/product.rs` | `Product`, `ProductStatus`, `UnitOfMeasure` |
| `src/models/category.rs` | `Category` |
| `src/models/product_variant.rs` | `ProductVariant` |
| `src/models/price_history.rs` | `PriceHistory`, `PriceType` |

Run the model tests with `cargo test`.
