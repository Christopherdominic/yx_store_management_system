//! Product catalogue: categories, products, variants and price history.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{Money, RecordStatus};
use super::ids::{
    BusinessId, PriceChangeId, ProductCategoryId, ProductId, ProductVariantId, UserId,
};

/// A product category, optionally nested under a parent (SRS FR-08).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductCategory {
    pub id: ProductCategoryId,
    pub business_id: BusinessId,
    pub parent_id: Option<ProductCategoryId>,
    pub name: String,
    pub status: RecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A catalogue item shared by every store in the business (SRS FR-07).
///
/// Products without variants are stocked and sold directly. Products with
/// variants are stocked and sold through each [`ProductVariant`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub id: ProductId,
    pub business_id: BusinessId,
    pub category_id: Option<ProductCategoryId>,
    pub name: String,
    pub description: Option<String>,
    /// Unique within the business.
    pub sku: String,
    pub barcode: Option<String>,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub unit: UnitOfMeasure,
    pub cost_price: Money,
    pub selling_price: Money,
    /// Whether stock is tracked per batch with expiry dates (SRS FR-14).
    pub tracks_batches: bool,
    pub status: RecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Product {
    pub fn is_active(&self) -> bool {
        self.status == RecordStatus::Active
    }
}

/// An independently stocked version of a product, such as a size or colour
/// (SRS FR-08).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductVariant {
    pub id: ProductVariantId,
    pub product_id: ProductId,
    /// Display name, e.g. `"500g"` or `"Large / Red"`.
    pub name: String,
    /// Attribute values, e.g. `{"size": "L", "colour": "Red"}`.
    pub attributes: BTreeMap<String, String>,
    /// Unique within the business.
    pub sku: String,
    pub barcode: Option<String>,
    /// Overrides the product's cost price when set.
    pub cost_price: Option<Money>,
    /// Overrides the product's selling price when set.
    pub selling_price: Option<Money>,
    pub status: RecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProductVariant {
    pub fn effective_cost_price(&self, product: &Product) -> Money {
        self.cost_price.unwrap_or(product.cost_price)
    }

    pub fn effective_selling_price(&self, product: &Product) -> Money {
        self.selling_price.unwrap_or(product.selling_price)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitOfMeasure {
    Piece,
    Pack,
    Carton,
    Bag,
    Kilogram,
    Gram,
    Litre,
    Millilitre,
    Metre,
}

impl UnitOfMeasure {
    /// Whether quantities in this unit may be fractional (e.g. 1.5 kg).
    pub fn allows_fractional(self) -> bool {
        matches!(
            self,
            Self::Kilogram | Self::Gram | Self::Litre | Self::Millilitre | Self::Metre
        )
    }
}

/// An immutable record of a price change (SRS FR-09).
///
/// Completed sales keep their own unit prices, so price changes never alter
/// historical totals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceChange {
    pub id: PriceChangeId,
    pub product_id: ProductId,
    /// Set when the change applies to a single variant.
    pub variant_id: Option<ProductVariantId>,
    pub previous_cost_price: Money,
    pub new_cost_price: Money,
    pub previous_selling_price: Money,
    pub new_selling_price: Money,
    pub effective_from: DateTime<Utc>,
    pub reason: Option<String>,
    pub changed_by: UserId,
    pub changed_at: DateTime<Utc>,
}
