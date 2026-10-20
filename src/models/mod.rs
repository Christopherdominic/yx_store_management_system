//! Domain models.
//!
//! Only modules that compile are declared here. The other model files in this
//! folder are drafts owned by their respective feature tasks and are added
//! once they are completed.

// Product management (SRS FR-07 – FR-09).
pub mod category;
pub mod price_history;
pub mod product;
pub mod product_variant;

pub use category::Category;
pub use price_history::{PriceHistory, PriceType};
pub use product::{Product, ProductStatus, UnitOfMeasure};
pub use product_variant::ProductVariant;
