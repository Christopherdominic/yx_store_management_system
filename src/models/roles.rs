use serde::Serialize;

use super::text_enum;

// Business-wide roles apply to every store in the business.
text_enum!(BusinessRole {
    BusinessOwner => "business_owner",
    Administrator => "administrator",
});

// Store roles apply only to the store they are assigned to (FR-05).
text_enum!(StoreRole {
    StoreManager => "store_manager",
    Cashier => "cashier",
    InventoryOfficer => "inventory_officer",
    FinanceOfficer => "finance_officer",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    /// Grant or revoke the business_owner / administrator roles.
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

impl BusinessRole {
    pub fn permissions(self) -> &'static [Permission] {
        match self {
            Self::BusinessOwner => &[
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
            Self::Administrator => &[
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
        }
    }
}

impl StoreRole {
    pub fn permissions(self) -> &'static [Permission] {
        match self {
            Self::StoreManager => &[
                BusinessView,
                StoreView,
                StoreUpdate,
                StoreSettingsUpdate,
                EmployeeView,
            ],
            Self::Cashier | Self::InventoryOfficer | Self::FinanceOfficer => {
                &[BusinessView, StoreView]
            }
        }
    }
}
