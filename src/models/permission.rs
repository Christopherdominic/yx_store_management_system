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
    /// View categories, products, variants and price history (FR-07 – FR-09).
    ProductView,
    /// Create, edit, archive and restore categories, products and variants.
    ProductManage,
    /// Change cost and selling prices (FR-09).
    PriceManage,
}

impl Permission {
    /// Permissions a store role may hold for its own store. Everything else
    /// requires a business-wide role.
    ///
    /// The product catalogue is shared by every store in the business, so
    /// product permissions held at any store apply to the whole catalogue.
    pub fn is_store_scoped(self) -> bool {
        matches!(
            self,
            Self::StoreView
                | Self::StoreUpdate
                | Self::StoreSettingsUpdate
                | Self::EmployeeView
                | Self::ProductView
                | Self::ProductManage
                | Self::PriceManage
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
                ProductView,
                ProductManage,
                PriceManage,
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
                ProductView,
                ProductManage,
                PriceManage,
            ],
            Role::StoreManager => &[
                BusinessView,
                StoreView,
                StoreUpdate,
                StoreSettingsUpdate,
                EmployeeView,
                ProductView,
                ProductManage,
                PriceManage,
            ],
            Role::InventoryOfficer => &[BusinessView, StoreView, ProductView, ProductManage],
            Role::Cashier | Role::FinanceOfficer => &[BusinessView, StoreView, ProductView],
        }
    }

    pub fn has(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }
}
