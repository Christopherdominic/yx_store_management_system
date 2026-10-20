//! Strongly typed identifiers.
//!
//! Each entity gets its own ID type wrapping a UUID v7. v7 UUIDs are
//! time-ordered, which keeps database indexes compact while still being
//! globally unique and safe to generate on the application side.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_ids {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a new, unique, time-ordered identifier.
            pub fn generate() -> Self {
                Self(Uuid::now_v7())
            }

            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(uuid: Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    )+};
}

define_ids!(
    BusinessId,
    StoreId,
    UserId,
    RoleAssignmentId,
    CustomerId,
    ProductCategoryId,
    ProductId,
    ProductVariantId,
    PriceHistoryId,
    InventoryItemId,
    StockMovementId,
    StockTransferId,
    StockTransferItemId,
    SaleId,
    SaleItemId,
    PaymentId,
    ExpenseCategoryId,
    ExpenseId,
    InventoryBatchId,
    StockCountId,
    StockCountItemId,
    StockAdjustmentId,
    StockAdjustmentItemId,
    SupplierId,
    SupplierProductId,
    SupplierLedgerEntryId,
    PurchaseOrderId,
    PurchaseOrderItemId,
    GoodsReceiptId,
    GoodsReceiptItemId,
    PurchaseReturnId,
    PurchaseReturnItemId,
    SaleReturnId,
    SaleReturnItemId,
    RefundId,
    NotificationId,
    AuditLogId,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_plain_uuid_string() {
        let id = StoreId::generate();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{}\"", id.as_uuid()));

        let back: StoreId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn generated_ids_are_unique_and_ordered() {
        let a = ProductId::generate();
        let b = ProductId::generate();
        assert_ne!(a, b);
        assert!(a < b);
    }
}
