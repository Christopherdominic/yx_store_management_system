//! Product catalogue entries (SRS FR-07).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{Money, RecordStatus};
use super::ids::{BusinessId, ProductCategoryId, ProductId};

/// Business-owned catalogue entry, shared by every store in the business.
///
/// A product without variants is stocked and sold directly. A product with
/// variants is stocked and sold through each
/// [`ProductVariant`](super::ProductVariant), and its own SKU and prices act
/// as catalogue defaults.
///
/// Prices are in the business currency. Products referenced by history are
/// archived, never deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    pub id: ProductId,
    pub business_id: BusinessId,
    pub category_id: Option<ProductCategoryId>,
    pub name: String,
    pub description: Option<String>,
    /// Unique across products and variants within the business.
    pub sku: String,
    pub barcode: Option<String>,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub unit_of_measure: UnitOfMeasure,
    pub cost_price: Money,
    pub selling_price: Money,
    /// Extra descriptive attributes, e.g. `{"origin": "Nigeria"}`.
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    /// Whether stock is tracked per batch with expiry dates (SRS FR-14).
    pub tracks_batches: bool,
    pub status: RecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Product {
    /// Archived products stay readable but cannot be sold or restocked.
    pub fn is_active(&self) -> bool {
        self.status == RecordStatus::Active
    }

    /// Selling price minus cost price per unit.
    pub fn unit_margin(&self) -> Money {
        self.selling_price - self.cost_price
    }
}

/// Unit that stock and sale quantities are measured in (SRS FR-10).
/// Variants use their parent product's unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnitOfMeasure {
    #[default]
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn product() -> Product {
        Product {
            id: ProductId::generate(),
            business_id: BusinessId::generate(),
            category_id: Some(ProductCategoryId::generate()),
            name: "Rice 50kg".into(),
            description: None,
            sku: "RICE-50".into(),
            barcode: Some("6151234567890".into()),
            brand: None,
            image_url: None,
            unit_of_measure: UnitOfMeasure::Bag,
            cost_price: dec!(65000.00),
            selling_price: dec!(72500.50),
            attributes: BTreeMap::new(),
            tracks_batches: false,
            status: RecordStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn prices_serialize_as_exact_strings() {
        let json = serde_json::to_value(product()).unwrap();
        assert_eq!(json["selling_price"], "72500.50");
        assert_eq!(json["cost_price"], "65000.00");
        assert_eq!(json["unit_of_measure"], "bag");
        assert_eq!(json["status"], "active");
    }

    #[test]
    fn round_trips_through_json() {
        let original = product();
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(serde_json::from_str::<Product>(&json).unwrap(), original);
    }

    #[test]
    fn archived_products_are_inactive() {
        let mut p = product();
        assert!(p.is_active());
        p.status = RecordStatus::Archived;
        assert!(!p.is_active());
    }

    #[test]
    fn unit_margin_is_exact() {
        assert_eq!(product().unit_margin(), dec!(7500.50));
    }

    #[test]
    fn fractional_units() {
        assert!(UnitOfMeasure::Kilogram.allows_fractional());
        assert!(!UnitOfMeasure::Bag.allows_fractional());
    }
}
