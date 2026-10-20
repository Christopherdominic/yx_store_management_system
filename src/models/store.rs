use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use super::text_enum;

text_enum!(StoreStatus {
    Active => "active",
    Inactive => "inactive",
});

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Store {
    pub id: Uuid,
    pub business_id: Uuid,
    pub code: String,
    pub name: String,
    pub address: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub status: StoreStatus,
    pub receipt_prefix: String,
    pub transaction_prefix: String,
    pub receipt_footer: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_by: Uuid,
    pub updated_at: DateTime<Utc>,
}

/// Store-level operational preferences (FR-03).
#[derive(Debug, Clone, Serialize)]
pub struct StoreSettings {
    pub store_id: Uuid,
    pub receipt_prefix: String,
    pub transaction_prefix: String,
    pub receipt_footer: Option<String>,
}

impl From<&Store> for StoreSettings {
    fn from(s: &Store) -> Self {
        Self {
            store_id: s.id,
            receipt_prefix: s.receipt_prefix.clone(),
            transaction_prefix: s.transaction_prefix.clone(),
            receipt_footer: s.receipt_footer.clone(),
        }
    }
}
