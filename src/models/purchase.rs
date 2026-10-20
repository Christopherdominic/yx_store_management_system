//! Purchase orders, goods receipts and returns to suppliers
//! (SRS FR-21, FR-23).

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::common::{CurrencyCode, Money, Quantity};
use super::ids::{
    BusinessId, GoodsReceiptId, GoodsReceiptItemId, ProductId, ProductVariantId, PurchaseOrderId,
    PurchaseOrderItemId, PurchaseReturnId, PurchaseReturnItemId, StoreId, SupplierId, UserId,
};

/// An order placed with a supplier for delivery to one store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: PurchaseOrderId,
    pub business_id: BusinessId,
    pub supplier_id: SupplierId,
    /// Destination store.
    pub store_id: StoreId,
    /// Human-readable number, e.g. `LAG-PO-000078`.
    pub order_number: String,
    pub status: PurchaseOrderStatus,
    pub currency: CurrencyCode,
    pub subtotal: Money,
    /// Delivery, handling and other charges.
    pub additional_charges: Money,
    pub tax_total: Money,
    /// `subtotal + additional_charges + tax_total`.
    pub total: Money,
    pub expected_delivery_date: Option<NaiveDate>,
    pub notes: Option<String>,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub approved_by: Option<UserId>,
    pub approved_at: Option<DateTime<Utc>>,
    pub cancelled_by: Option<UserId>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancellation_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseOrderStatus {
    /// Editable; not yet sent for approval.
    #[default]
    Draft,
    Submitted,
    Approved,
    PartiallyReceived,
    Received,
    Cancelled,
}

impl PurchaseOrderStatus {
    pub fn is_editable(self) -> bool {
        self == Self::Draft
    }

    /// Goods can be received only against approved orders.
    pub fn can_receive(self) -> bool {
        matches!(self, Self::Approved | Self::PartiallyReceived)
    }

    /// Allowed status changes:
    ///
    /// ```text
    /// Draft ─► Submitted ─► Approved ─► PartiallyReceived ─► Received
    ///   │          │           │               │       └──────────┘
    ///   └──────────┴───────────┴───────────────┴─► Cancelled
    /// ```
    ///
    /// A submitted order can also go back to `Draft` for edits. Cancelling a
    /// partially received order cancels only its outstanding quantities.
    pub fn can_transition_to(self, next: Self) -> bool {
        use PurchaseOrderStatus::*;
        matches!(
            (self, next),
            (Draft, Submitted | Cancelled)
                | (Submitted, Draft | Approved | Cancelled)
                | (Approved, PartiallyReceived | Received | Cancelled)
                | (PartiallyReceived, PartiallyReceived | Received | Cancelled)
        )
    }
}

/// One product line on a purchase order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PurchaseOrderItem {
    pub id: PurchaseOrderItemId,
    pub purchase_order_id: PurchaseOrderId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub quantity_ordered: Quantity,
    pub quantity_received: Quantity,
    pub quantity_cancelled: Quantity,
    pub unit_cost: Money,
    /// `quantity_ordered * unit_cost`.
    pub line_total: Money,
}

impl PurchaseOrderItem {
    /// Quantity still expected from the supplier (never negative).
    pub fn quantity_outstanding(&self) -> Quantity {
        (self.quantity_ordered - self.quantity_received - self.quantity_cancelled)
            .max(Quantity::ZERO)
    }
}

/// A delivery of goods against a purchase order. Only a confirmed receipt
/// increases store stock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoodsReceipt {
    pub id: GoodsReceiptId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub supplier_id: SupplierId,
    pub purchase_order_id: PurchaseOrderId,
    pub receipt_number: String,
    pub status: GoodsReceiptStatus,
    pub supplier_invoice_number: Option<String>,
    pub notes: Option<String>,
    pub received_by: UserId,
    pub received_at: DateTime<Utc>,
    pub confirmed_by: Option<UserId>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoodsReceiptStatus {
    #[default]
    Draft,
    /// Stock has been added to the store.
    Confirmed,
    Cancelled,
}

/// One line on a goods receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoodsReceiptItem {
    pub id: GoodsReceiptItemId,
    pub goods_receipt_id: GoodsReceiptId,
    pub purchase_order_item_id: PurchaseOrderItemId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub quantity_received: Quantity,
    pub unit_cost: Money,
    /// Required for batch-tracked products.
    pub batch_number: Option<String>,
    pub expires_on: Option<NaiveDate>,
    /// Explanation when more than the outstanding quantity was accepted.
    pub over_receipt_reason: Option<String>,
}

/// Goods sent back to a supplier from a confirmed receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PurchaseReturn {
    pub id: PurchaseReturnId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub supplier_id: SupplierId,
    pub goods_receipt_id: GoodsReceiptId,
    pub return_number: String,
    pub status: PurchaseReturnStatus,
    pub currency: CurrencyCode,
    pub total: Money,
    pub reason: String,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub confirmed_by: Option<UserId>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseReturnStatus {
    #[default]
    Draft,
    /// Stock has been removed and the supplier balance updated.
    Confirmed,
    Cancelled,
}

/// One line on a purchase return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PurchaseReturnItem {
    pub id: PurchaseReturnItemId,
    pub purchase_return_id: PurchaseReturnId,
    pub goods_receipt_item_id: GoodsReceiptItemId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub quantity: Quantity,
    pub unit_cost: Money,
    pub line_total: Money,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::PurchaseOrderStatus::*;
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn outstanding_quantity_never_goes_negative() {
        let mut item = PurchaseOrderItem {
            id: PurchaseOrderItemId::generate(),
            purchase_order_id: PurchaseOrderId::generate(),
            product_id: ProductId::generate(),
            variant_id: None,
            quantity_ordered: dec!(100),
            quantity_received: dec!(60),
            quantity_cancelled: dec!(10),
            unit_cost: dec!(2500),
            line_total: dec!(250000),
        };
        assert_eq!(item.quantity_outstanding(), dec!(30));
        item.quantity_received = dec!(105);
        assert_eq!(item.quantity_outstanding(), dec!(0));
    }

    #[test]
    fn order_lifecycle() {
        assert!(Draft.can_transition_to(Submitted));
        assert!(Submitted.can_transition_to(Approved));
        assert!(Approved.can_transition_to(PartiallyReceived));
        assert!(PartiallyReceived.can_transition_to(Received));
        assert!(!Draft.can_transition_to(Approved));
        assert!(!Received.can_transition_to(Cancelled));
        assert!(!Cancelled.can_transition_to(Draft));
        assert!(!Submitted.can_receive());
        assert!(Approved.can_receive());
    }
}
