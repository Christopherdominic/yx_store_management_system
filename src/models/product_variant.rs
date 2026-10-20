use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::product::ProductStatus;

/// Independently stocked and sold variation of a product (SRS FR-08), such as
/// a size, colour, model or pack size.
///
/// Each variant has its own SKU, optional barcode and explicit prices.
/// Inventory and sale items reference the variant ID alongside the parent
/// product ID. The unit of measure comes from the parent product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductVariant {
    pub id: u64,
    pub business_id: u64,
    pub product_id: u64,
    /// Display name, e.g. `"Large / Red"`.
    pub name: String,
    /// Unique across products and variants within the business.
    pub sku: String,
    pub barcode: Option<String>,
    /// Variant-defining values, e.g. `{"size": "L", "colour": "Red"}`.
    pub attributes: BTreeMap<String, String>,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub selling_price: Decimal,
    pub status: ProductStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProductVariant {
    pub fn is_active(&self) -> bool {
        self.status == ProductStatus::Active
    }

    /// Selling price minus cost price per unit.
    pub fn unit_margin(&self) -> Decimal {
        self.selling_price - self.cost_price
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn attributes_serialize_in_stable_order() {
        let variant = ProductVariant {
            id: 2,
            business_id: 1,
            product_id: 1,
            name: "Large / Red".into(),
            sku: "TSHIRT-L-RED".into(),
            barcode: None,
            attributes: BTreeMap::from([
                ("size".into(), "L".into()),
                ("colour".into(), "Red".into()),
            ]),
            cost_price: dec!(3000),
            selling_price: dec!(4500),
            status: ProductStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&variant.attributes).unwrap();
        assert_eq!(json, r#"{"colour":"Red","size":"L"}"#);
        assert_eq!(variant.unit_margin(), dec!(1500));
    }
}
