//! Append-only audit trail (SRS FR-27).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::common::RecordRef;
use super::ids::{AuditLogId, BusinessId, StoreId, UserId};

/// One audited action. Entries are never edited or deleted.
///
/// `before` and `after` hold the changed fields only, and must never contain
/// passwords, tokens or other secrets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: AuditLogId,
    pub business_id: BusinessId,
    pub store_id: Option<StoreId>,
    pub actor: AuditActor,
    pub action: AuditAction,
    pub record: Option<RecordRef>,
    pub outcome: AuditOutcome,
    pub before: Option<Value>,
    pub after: Option<Value>,
    /// Free-text detail, e.g. an adjustment reason or denial cause.
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    /// Correlates all entries written by one API request.
    pub request_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

/// Who performed the action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum AuditActor {
    User(UserId),
    /// A background job or system process, by name.
    System(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Login,
    LoginFailed,
    Logout,
    PasswordChanged,
    PermissionChanged,
    AccessDenied,
    Created,
    Updated,
    Archived,
    Deactivated,
    Reactivated,
    Submitted,
    Approved,
    Rejected,
    Cancelled,
    Completed,
    Voided,
    PriceChanged,
    StockMoved,
    SettingsChanged,
    Imported,
    Exported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Failure,
    /// Blocked by an authorization check.
    Denied,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_serializes_with_type_tag() {
        let json = serde_json::to_value(AuditActor::System("low_stock_job".into())).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "system", "id": "low_stock_job"})
        );
    }
}
