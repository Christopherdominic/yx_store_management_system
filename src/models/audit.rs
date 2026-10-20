use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use super::text_enum;

text_enum!(AuditOutcome {
    Success => "success",
    Denied => "denied",
    Failure => "failure",
});

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub business_id: Option<Uuid>,
    pub store_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub outcome: AuditOutcome,
    pub before_values: Option<Value>,
    pub after_values: Option<Value>,
    pub metadata: Option<Value>,
    pub created_at: DateTime<Utc>,
}
