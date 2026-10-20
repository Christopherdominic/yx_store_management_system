//! Value types shared across models.

use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A monetary amount in major units (e.g. naira, not kobo), stored exactly.
/// The currency is carried by the owning record.
pub type Money = Decimal;

/// A stock quantity in the product's unit of measure. Decimal so that
/// weighed or measured goods (kg, litres) are supported.
pub type Quantity = Decimal;

/// Lifecycle of catalogue-style records that must never be hard-deleted
/// once referenced by transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecordStatus {
    #[default]
    Active,
    Archived,
}

/// ISO 4217 currency code, e.g. `NGN`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn new(code: &str) -> Result<Self, InvalidCurrencyCode> {
        let code = code.trim().to_ascii_uppercase();
        if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
            Ok(Self(code))
        } else {
            Err(InvalidCurrencyCode(code))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CurrencyCode {
    type Error = InvalidCurrencyCode;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<CurrencyCode> for String {
    fn from(code: CurrencyCode) -> Self {
        code.0
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidCurrencyCode(pub String);

impl fmt::Display for InvalidCurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid ISO 4217 currency code: {:?}", self.0)
    }
}

impl std::error::Error for InvalidCurrencyCode {}

/// The kinds of record that can be referenced generically, e.g. from a stock
/// movement, notification or audit entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordType {
    Business,
    Store,
    User,
    RoleAssignment,
    Customer,
    ProductCategory,
    Product,
    ProductVariant,
    PriceChange,
    InventoryItem,
    InventoryBatch,
    StockMovement,
    StockCount,
    StockAdjustment,
    StockTransfer,
    Sale,
    Payment,
    SaleReturn,
    Refund,
    Supplier,
    PurchaseOrder,
    GoodsReceipt,
    PurchaseReturn,
    ExpenseCategory,
    Expense,
}

/// A typed pointer to any record, used where the target type varies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordRef {
    pub record_type: RecordType,
    pub record_id: Uuid,
}

impl RecordRef {
    pub fn new(record_type: RecordType, record_id: impl Into<Uuid>) -> Self {
        Self {
            record_type,
            record_id: record_id.into(),
        }
    }
}

/// A postal address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Address {
    pub line1: String,
    pub line2: Option<String>,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    /// ISO 3166-1 alpha-2 country code, e.g. `NG`.
    pub country: String,
}

/// Contact details for a business, store, customer or supplier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContactInfo {
    pub phone: Option<String>,
    pub email: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_code_is_normalized() {
        assert_eq!(CurrencyCode::new(" ngn ").unwrap().as_str(), "NGN");
    }

    #[test]
    fn currency_code_rejects_invalid_values() {
        assert!(CurrencyCode::new("NG").is_err());
        assert!(CurrencyCode::new("NGN1").is_err());
        assert!(CurrencyCode::new("N1N").is_err());
        assert!(serde_json::from_str::<CurrencyCode>("\"US\"").is_err());
    }
}
