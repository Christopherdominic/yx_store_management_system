//! API view of a staff member with their active roles (SRS FR-05).

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use super::ids::StoreId;
use super::user::{Role, User};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Employee {
    #[serde(flatten)]
    pub user: User,
    /// Active business-wide role, if any.
    pub business_role: Option<Role>,
    /// Active per-store roles.
    pub stores: Vec<EmployeeStoreRole>,
}

#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct EmployeeStoreRole {
    pub store_id: StoreId,
    pub store_name: String,
    pub store_code: String,
    pub role: Role,
    pub assigned_at: DateTime<Utc>,
}
