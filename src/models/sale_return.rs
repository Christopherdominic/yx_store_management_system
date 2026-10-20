//! Customer returns and refunds (SRS FR-18).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{CurrencyCode, Money, Quantity};
use super::ids::{
    BusinessId, ProductId, ProductVariantId, RefundId, SaleId, SaleItemId, SaleReturnId,
    SaleReturnItemId, StoreId, UserId,
};
use super::sale::PaymentMethod;

/// A full or partial return against an original sale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaleReturn {
    pub id: SaleReturnId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub sale_id: SaleId,
    pub return_number: String,
    pub status: SaleReturnStatus,
    pub reason: String,
    pub currency: CurrencyCode,
    /// Sum of item refund amounts.
    pub refund_total: Money,
    pub processed_by: UserId,
    pub created_at: DateTime<Utc>,
    pub approved_by: Option<UserId>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SaleReturnStatus {
    #[default]
    PendingApproval,
    Approved,
    Rejected,
    /// Stock and refund have been processed.
    Completed,
}

/// One returned line, linked to the original sale item so the system can stop
/// more units being returned than were sold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaleReturnItem {
    pub id: SaleReturnItemId,
    pub sale_return_id: SaleReturnId,
    pub sale_item_id: SaleItemId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub quantity: Quantity,
    pub condition: ItemCondition,
    /// Whether the item goes back into saleable stock.
    pub restock: bool,
    pub refund_amount: Money,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemCondition {
    Resaleable,
    Damaged,
    Defective,
    Expired,
}

impl ItemCondition {
    /// Only resaleable items may re-enter saleable stock.
    pub fn can_restock(self) -> bool {
        self == Self::Resaleable
    }
}

/// Money paid back to a customer, recorded separately from the original
/// payments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refund {
    pub id: RefundId,
    pub business_id: BusinessId,
    pub sale_id: SaleId,
    pub sale_return_id: Option<SaleReturnId>,
    pub method: PaymentMethod,
    pub amount: Money,
    pub currency: CurrencyCode,
    pub status: RefundStatus,
    pub reason: String,
    pub external_reference: Option<String>,
    /// Prevents duplicate refunds on retried requests (SRS NFR-10).
    pub idempotency_key: Option<String>,
    pub processed_by: UserId,
    pub approved_by: Option<UserId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RefundStatus {
    #[default]
    Pending,
    Successful,
    Failed,
}
