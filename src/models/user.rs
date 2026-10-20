use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use super::roles::{BusinessRole, StoreRole};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub phone: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_active: bool,
    #[serde(skip_serializing)]
    pub failed_login_attempts: i32,
    #[serde(skip_serializing)]
    pub locked_until: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct StoreAssignment {
    pub store_id: Uuid,
    pub store_name: String,
    pub store_code: String,
    pub role: StoreRole,
}

/// A user as seen from inside one business.
#[derive(Debug, Clone, Serialize)]
pub struct Employee {
    pub user_id: Uuid,
    pub business_id: Uuid,
    pub email: String,
    pub full_name: String,
    pub phone: Option<String>,
    pub business_role: Option<BusinessRole>,
    /// False when deactivated in this business.
    pub is_active: bool,
    pub stores: Vec<StoreAssignment>,
    pub created_at: DateTime<Utc>,
}

/// A business the current user belongs to, with their roles in it.
#[derive(Debug, Clone, Serialize)]
pub struct Membership {
    pub business_id: Uuid,
    pub business_name: String,
    pub business_role: Option<BusinessRole>,
    pub stores: Vec<StoreAssignment>,
}
