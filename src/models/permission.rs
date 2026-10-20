//! What each [`Role`] may do (SRS FR-05, FR-06).

use serde::Serialize;

use super::user::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    BusinessView,
    BusinessUpdate,
    BusinessSettingsUpdate,
    StoreCreate,
    StoreView,
    StoreUpdate,
    StoreStatusChange,
    StoreSettingsUpdate,
    EmployeeView,
    EmployeeManage,
    /// Grant or revoke the business owner / administrator roles.
    BusinessRoleManage,
    AuditLogView,
}

impl Permission {
    /// Permissions a store role may hold for its own store. Everything else
    /// requires a business-wide role.
    pub fn is_store_scoped(self) -> bool {
        matches!(
            self,
            Self::StoreView | Self::StoreUpdate | Self::StoreSettingsUpdate | Self::EmployeeView
        )
    }
}

use Permission::*;

impl Role {
    pub const ALL: &'static [Role] = &[
        Role::BusinessOwner,
        Role::Administrator,
        Role::StoreManager,
        Role::Cashier,
        Role::InventoryOfficer,
        Role::FinanceOfficer,
    ];

    pub fn permissions(self) -> &'static [Permission] {
        match self {
            Role::BusinessOwner => &[
                BusinessView,
                BusinessUpdate,
                BusinessSettingsUpdate,
                StoreCreate,
                StoreView,
                StoreUpdate,
                StoreStatusChange,
                StoreSettingsUpdate,
                EmployeeView,
                EmployeeManage,
                BusinessRoleManage,
                AuditLogView,
            ],
            Role::Administrator => &[
                BusinessView,
                BusinessUpdate,
                BusinessSettingsUpdate,
                StoreCreate,
                StoreView,
                StoreUpdate,
                StoreStatusChange,
                StoreSettingsUpdate,
                EmployeeView,
                EmployeeManage,
                AuditLogView,
            ],
            Role::StoreManager => &[
                BusinessView,
                StoreView,
                StoreUpdate,
                StoreSettingsUpdate,
                EmployeeView,
            ],
            Role::Cashier | Role::InventoryOfficer | Role::FinanceOfficer => {
                &[BusinessView, StoreView]
            }
        }
    }

    pub fn has(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }
}
