//! Physical stock counts and stock adjustments (SRS FR-12).
//!
//! A count records what is physically on the shelf. Differences become a
//! proposed [`StockAdjustment`], and stock changes only once that adjustment
//! is approved and posted as stock movements.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::Quantity;
use super::ids::{
    BusinessId, InventoryBatchId, ProductId, ProductVariantId, StockAdjustmentId,
    StockAdjustmentItemId, StockCountId, StockCountItemId, StoreId, UserId,
};

/// A physical stock count at one store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockCount {
    pub id: StockCountId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    /// Human-readable number, e.g. `LAG-CNT-000045`.
    pub count_number: String,
    pub status: StockCountStatus,
    pub notes: Option<String>,
    pub started_by: UserId,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    /// The adjustment raised from this count's differences.
    pub adjustment_id: Option<StockAdjustmentId>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StockCountStatus {
    #[default]
    InProgress,
    Completed,
    Cancelled,
}

/// One counted line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockCountItem {
    pub id: StockCountItemId,
    pub stock_count_id: StockCountId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub batch_id: Option<InventoryBatchId>,
    /// System balance when the line was counted.
    pub expected_quantity: Quantity,
    /// `None` until the line has been counted.
    pub counted_quantity: Option<Quantity>,
    pub counted_by: Option<UserId>,
    pub counted_at: Option<DateTime<Utc>>,
}

impl StockCountItem {
    /// `counted - expected`; negative means stock is missing.
    pub fn variance(&self) -> Option<Quantity> {
        self.counted_quantity
            .map(|counted| counted - self.expected_quantity)
    }
}

/// A request to correct stock levels, raised manually or from a count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockAdjustment {
    pub id: StockAdjustmentId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub adjustment_number: String,
    pub kind: AdjustmentKind,
    /// Set when raised from a stock count.
    pub stock_count_id: Option<StockCountId>,
    /// Mandatory explanation for the adjustment.
    pub reason: String,
    /// Supporting document, e.g. an incident report number.
    pub reference: Option<String>,
    pub status: AdjustmentStatus,
    pub requested_by: UserId,
    pub requested_at: DateTime<Utc>,
    /// Who approved or rejected it.
    pub reviewed_by: Option<UserId>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    /// When the resulting stock movements were written.
    pub posted_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentKind {
    /// Corrects a recording error or count difference.
    Correction,
    /// Removes damaged, expired or lost stock.
    WriteOff,
    /// Moves saleable stock into the unsellable bucket.
    MarkUnsellable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentStatus {
    #[default]
    PendingApproval,
    Approved,
    Rejected,
    /// Approved and applied to stock.
    Posted,
}

/// One product line on an adjustment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockAdjustmentItem {
    pub id: StockAdjustmentItemId,
    pub adjustment_id: StockAdjustmentId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub batch_id: Option<InventoryBatchId>,
    /// Signed change to apply: positive adds stock, negative removes it.
    pub quantity_change: Quantity,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn variance_is_counted_minus_expected() {
        let mut item = StockCountItem {
            id: StockCountItemId::generate(),
            stock_count_id: StockCountId::generate(),
            product_id: ProductId::generate(),
            variant_id: None,
            batch_id: None,
            expected_quantity: dec!(20),
            counted_quantity: None,
            counted_by: None,
            counted_at: None,
        };
        assert_eq!(item.variance(), None);
        item.counted_quantity = Some(dec!(17));
        assert_eq!(item.variance(), Some(dec!(-3)));
    }
}
