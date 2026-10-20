//! Staff accounts and their role assignments.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::ids::{BusinessId, RoleAssignmentId, StoreId, UserId};

/// A staff account (SRS FR-04, FR-05).
///
/// Credentials (password hashes, sessions) are kept in separate auth records
/// so they can never be serialized into an API response by accident.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub business_id: BusinessId,
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub status: UserStatus,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn is_active(&self) -> bool {
        self.status == UserStatus::Active
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    #[default]
    Active,
    Deactivated,
}

/// Roles defined by SRS FR-05.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    BusinessOwner,
    Administrator,
    StoreManager,
    Cashier,
    InventoryOfficer,
    FinanceOfficer,
}

impl Role {
    /// Business-wide roles apply to every store and are assigned without a
    /// store. All other roles must be assigned per store.
    pub fn is_business_wide(self) -> bool {
        matches!(self, Role::BusinessOwner | Role::Administrator)
    }
}

/// Grants a user a role, either across the business or at one store.
///
/// A user may hold different roles at different stores, e.g. manager in
/// Lagos and cashier in Abuja (SRS FR-05).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleAssignment {
    pub id: RoleAssignmentId,
    pub business_id: BusinessId,
    pub user_id: UserId,
    /// `None` for business-wide roles.
    pub store_id: Option<StoreId>,
    pub role: Role,
    pub assigned_by: UserId,
    pub assigned_at: DateTime<Utc>,
    /// Set when the assignment is revoked; revoked rows are kept for audit.
    pub revoked_at: Option<DateTime<Utc>>,
}

impl RoleAssignment {
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Whether this assignment grants its role at the given store.
    pub fn applies_to_store(&self, store_id: StoreId) -> bool {
        self.is_active() && self.store_id.is_none_or(|id| id == store_id)
    }
}
