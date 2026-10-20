//! Suppliers, the products they supply, and supplier balances (SRS FR-20, FR-23).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{Address, ContactInfo, CurrencyCode, Money, RecordRef};
use super::ids::{
    BusinessId, ProductId, ProductVariantId, SupplierId, SupplierLedgerEntryId, SupplierProductId,
    UserId,
};

/// A supplier the business buys from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Supplier {
    pub id: SupplierId,
    pub business_id: BusinessId,
    /// Short code unique within the business.
    pub code: String,
    pub name: String,
    pub contact_person: Option<String>,
    pub contact: ContactInfo,
    pub address: Option<Address>,
    /// Days allowed for payment on credit purchases. `None` means pay on
    /// delivery.
    pub payment_terms_days: Option<u32>,
    pub tax_id: Option<String>,
    pub notes: Option<String>,
    pub status: SupplierStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Supplier {
    pub fn is_active(&self) -> bool {
        self.status == SupplierStatus::Active
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SupplierStatus {
    #[default]
    Active,
    Inactive,
}

/// A product a supplier can supply, with its supplier-side details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplierProduct {
    pub id: SupplierProductId,
    pub supplier_id: SupplierId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    /// The supplier's own code for this product.
    pub supplier_sku: Option<String>,
    pub last_unit_cost: Option<Money>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An entry in a supplier's running balance, used when credit purchasing is
/// enabled.
///
/// Positive amounts increase what the business owes the supplier; negative
/// amounts reduce it. The balance is the sum of all entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplierLedgerEntry {
    pub id: SupplierLedgerEntryId,
    pub business_id: BusinessId,
    pub supplier_id: SupplierId,
    pub entry_type: SupplierLedgerEntryType,
    pub amount: Money,
    pub currency: CurrencyCode,
    /// The goods receipt, purchase return or payment behind this entry.
    pub source: Option<RecordRef>,
    pub notes: Option<String>,
    pub recorded_by: UserId,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierLedgerEntryType {
    /// Goods received on credit.
    Purchase,
    /// Payment made to the supplier.
    Payment,
    /// Credit from goods returned to the supplier.
    ReturnCredit,
    /// Manual correction.
    Adjustment,
}
