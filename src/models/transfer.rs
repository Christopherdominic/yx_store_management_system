//! Inter-store stock transfers (SRS FR-22).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::Quantity;
use super::ids::{
    BusinessId, ProductId, ProductVariantId, StockTransferId, StockTransferItemId, StoreId, UserId,
};

/// A request to move stock from one store to another.
///
/// Source stock leaves at dispatch and destination stock arrives at receipt,
/// so goods are never counted as available at both stores while in transit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockTransfer {
    pub id: StockTransferId,
    pub business_id: BusinessId,
    /// Human-readable number, e.g. `LAG-TRF-000123`.
    pub transfer_number: String,
    pub source_store_id: StoreId,
    pub destination_store_id: StoreId,
    pub status: TransferStatus,
    pub notes: Option<String>,
    pub requested_by: UserId,
    pub requested_at: DateTime<Utc>,
    pub approved_by: Option<UserId>,
    pub approved_at: Option<DateTime<Utc>>,
    pub dispatched_by: Option<UserId>,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub received_by: Option<UserId>,
    pub received_at: Option<DateTime<Utc>>,
    /// Who rejected or cancelled the transfer, and why.
    pub closed_by: Option<UserId>,
    pub closed_reason: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// One product line on a transfer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockTransferItem {
    pub id: StockTransferItemId,
    pub transfer_id: StockTransferId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub quantity_requested: Quantity,
    pub quantity_dispatched: Option<Quantity>,
    pub quantity_received: Option<Quantity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    #[default]
    Pending,
    Approved,
    Rejected,
    InTransit,
    Completed,
    Cancelled,
}

impl TransferStatus {
    /// Whether no further changes are allowed.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Rejected | Self::Completed | Self::Cancelled)
    }

    /// Allowed status changes:
    ///
    /// ```text
    /// Pending ─► Approved ─► InTransit ─► Completed
    ///    │          │
    ///    ├► Rejected └► Cancelled
    ///    └► Cancelled
    /// ```
    pub fn can_transition_to(self, next: Self) -> bool {
        use TransferStatus::*;
        matches!(
            (self, next),
            (Pending, Approved | Rejected | Cancelled)
                | (Approved, InTransit | Cancelled)
                | (InTransit, Completed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::TransferStatus::*;

    #[test]
    fn follows_the_documented_lifecycle() {
        assert!(Pending.can_transition_to(Approved));
        assert!(Approved.can_transition_to(InTransit));
        assert!(InTransit.can_transition_to(Completed));
    }

    #[test]
    fn rejects_skipped_or_reversed_steps() {
        assert!(!Pending.can_transition_to(InTransit));
        assert!(!Pending.can_transition_to(Completed));
        assert!(!InTransit.can_transition_to(Cancelled));
        assert!(!Completed.can_transition_to(Pending));
    }

    #[test]
    fn terminal_states_allow_no_transitions() {
        for terminal in [Rejected, Completed, Cancelled] {
            assert!(terminal.is_terminal());
            for next in [Pending, Approved, Rejected, InTransit, Completed, Cancelled] {
                assert!(!terminal.can_transition_to(next));
            }
        }
    }
}
