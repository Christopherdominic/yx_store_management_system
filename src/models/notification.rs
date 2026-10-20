//! In-app and optional email/SMS notifications (SRS FR-13, FR-26).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::RecordRef;
use super::ids::{BusinessId, NotificationId, StoreId, UserId};

/// A notification for one recipient.
///
/// Delivery failures never roll back the business transaction that raised
/// the notification (SRS NFR-11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: NotificationId,
    pub business_id: BusinessId,
    pub store_id: Option<StoreId>,
    pub recipient_id: UserId,
    pub event: NotificationEvent,
    pub channel: NotificationChannel,
    pub title: String,
    pub message: String,
    /// The record the recipient should open to act on this notification.
    pub record: Option<RecordRef>,
    /// Stops duplicate alerts for the same unresolved condition, e.g.
    /// `low_stock:<store>:<product>`.
    pub dedup_key: Option<String>,
    pub status: NotificationStatus,
    pub delivery_attempts: u32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEvent {
    LowStock,
    NearExpiry,
    TransferRequested,
    TransferApproved,
    TransferRejected,
    TransferDispatched,
    TransferReceived,
    PurchaseOrderApprovalRequired,
    PurchaseOrderApproved,
    StockAdjustmentApprovalRequired,
    ExpenseApprovalRequired,
    RefundApprovalRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    #[default]
    InApp,
    Email,
    Sms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotificationStatus {
    /// Queued for email/SMS delivery.
    Pending,
    /// Delivered but not yet opened.
    #[default]
    Unread,
    Read,
    /// Delivery failed after all retries.
    Failed,
}
