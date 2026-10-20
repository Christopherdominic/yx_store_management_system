//! Store-level stock balances and the movement ledger behind them.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::common::{Quantity, RecordRef};
use super::ids::{
    BusinessId, GoodsReceiptItemId, InventoryBatchId, InventoryItemId, ProductId, ProductVariantId,
    StockMovementId, StoreId, UserId,
};

/// The stock balance of one product (or variant) at one store (SRS FR-10).
///
/// Balances change only through the inventory service, which writes a
/// matching [`StockMovement`] in the same database transaction (SRS FR-11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: InventoryItemId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub quantity_on_hand: Quantity,
    /// Held for pending transfers or orders; not available for sale.
    pub quantity_reserved: Quantity,
    /// Damaged, expired or otherwise unsellable stock, kept separate from
    /// saleable stock (SRS FR-14).
    pub quantity_unsellable: Quantity,
    /// Low-stock threshold for this store (SRS FR-13).
    pub reorder_level: Option<Quantity>,
    /// Incremented on every update for optimistic concurrency control, so two
    /// concurrent sales cannot both consume the last unit (SRS NFR-09).
    pub version: i64,
    pub updated_at: DateTime<Utc>,
}

impl InventoryItem {
    /// Stock that can be sold or transferred right now.
    pub fn quantity_available(&self) -> Quantity {
        self.quantity_on_hand - self.quantity_reserved
    }

    /// Whether available stock is at or below the reorder level.
    pub fn is_low_stock(&self) -> bool {
        self.reorder_level
            .is_some_and(|level| self.quantity_available() <= level)
    }
}

/// An immutable ledger entry for a single stock change (SRS FR-11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockMovement {
    pub id: StockMovementId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    /// Set for batch-tracked products.
    pub batch_id: Option<InventoryBatchId>,
    pub movement_type: StockMovementType,
    /// Signed change: positive for stock in, negative for stock out.
    pub quantity_change: Quantity,
    /// On-hand balance immediately after this movement.
    pub balance_after: Quantity,
    /// The record that caused this movement (sale, transfer, goods receipt,
    /// adjustment, ...).
    pub source: Option<RecordRef>,
    /// Required for adjustments and write-offs (SRS FR-12).
    pub reason: Option<String>,
    /// `None` when the movement was made by a system process.
    pub performed_by: Option<UserId>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementType {
    OpeningBalance,
    Sale,
    SaleReturn,
    PurchaseReceipt,
    PurchaseReturn,
    TransferOut,
    TransferIn,
    Adjustment,
    WriteOff,
}

impl StockMovementType {
    /// Movements that must include a reason and may need approval.
    pub fn requires_reason(self) -> bool {
        matches!(self, Self::Adjustment | Self::WriteOff)
    }
}

/// A batch or lot of a batch-tracked product at one store (SRS FR-14).
///
/// The batch quantities add up to the store's [`InventoryItem`] balance.
/// Sales draw from the batch that expires first unless policy says otherwise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryBatch {
    pub id: InventoryBatchId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    /// Supplier or manufacturer lot number.
    pub batch_number: String,
    pub manufactured_on: Option<NaiveDate>,
    pub expires_on: Option<NaiveDate>,
    pub quantity_on_hand: Quantity,
    pub quantity_unsellable: Quantity,
    /// The goods receipt line this batch arrived on, if any.
    pub goods_receipt_item_id: Option<GoodsReceiptItemId>,
    pub received_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl InventoryBatch {
    /// Expired on or before `today`.
    pub fn is_expired(&self, today: NaiveDate) -> bool {
        self.expires_on.is_some_and(|date| date <= today)
    }

    /// Not yet expired, but expiring within `warning_days` of `today`.
    pub fn is_near_expiry(&self, today: NaiveDate, warning_days: u32) -> bool {
        self.expires_on.is_some_and(|date| {
            date > today && date <= today + chrono::Days::new(warning_days.into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn batch(expires_on: Option<NaiveDate>) -> InventoryBatch {
        InventoryBatch {
            id: InventoryBatchId::generate(),
            business_id: BusinessId::generate(),
            store_id: StoreId::generate(),
            product_id: ProductId::generate(),
            variant_id: None,
            batch_number: "LOT-1".into(),
            manufactured_on: None,
            expires_on,
            quantity_on_hand: dec!(10),
            quantity_unsellable: dec!(0),
            goods_receipt_item_id: None,
            received_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn batch_expiry_checks() {
        let today = date(2026, 10, 20);
        let b = batch(Some(date(2026, 10, 30)));
        assert!(!b.is_expired(today));
        assert!(b.is_near_expiry(today, 10));
        assert!(!b.is_near_expiry(today, 9));
        assert!(b.is_expired(date(2026, 10, 30)));
        assert!(!b.is_near_expiry(date(2026, 10, 30), 10));
        assert!(!batch(None).is_expired(today));
    }

    fn item(on_hand: Quantity, reserved: Quantity, reorder: Option<Quantity>) -> InventoryItem {
        InventoryItem {
            id: InventoryItemId::generate(),
            business_id: BusinessId::generate(),
            store_id: StoreId::generate(),
            product_id: ProductId::generate(),
            variant_id: None,
            quantity_on_hand: on_hand,
            quantity_reserved: reserved,
            quantity_unsellable: dec!(0),
            reorder_level: reorder,
            version: 0,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn available_excludes_reserved_stock() {
        assert_eq!(item(dec!(20), dec!(5), None).quantity_available(), dec!(15));
    }

    #[test]
    fn low_stock_is_inclusive_of_reorder_level() {
        assert!(item(dec!(10), dec!(0), Some(dec!(10))).is_low_stock());
        assert!(item(dec!(12), dec!(3), Some(dec!(10))).is_low_stock());
        assert!(!item(dec!(11), dec!(0), Some(dec!(10))).is_low_stock());
    }

    #[test]
    fn no_reorder_level_means_never_low() {
        assert!(!item(dec!(0), dec!(0), None).is_low_stock());
    }
}
