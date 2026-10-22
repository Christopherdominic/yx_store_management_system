//! Domain models for the multi-store retail management system.
//!
//! Conventions shared by every model:
//! - Identifiers are strongly typed UUID v7 wrappers (see [`ids`]), so a
//!   `StoreId` can never be passed where a `ProductId` is expected.
//! - Money and quantities use exact decimals ([`Money`], [`Quantity`]);
//!   floating point is never used for financial values (SRS NFR-30).
//! - Timestamps are stored in UTC. Business-local display uses the
//!   business timezone.
//! - Records referenced by history are archived or deactivated, never deleted.

pub mod adjustment;
pub mod audit;
pub mod business;
pub mod category;
pub mod common;
pub mod customer;
pub mod employee;
pub mod expense;
pub mod ids;
pub mod inventory;
pub mod notification;
pub mod permission;
pub mod price_history;
pub mod product;
pub mod product_variant;
pub mod purchase;
pub mod sale;
pub mod sale_return;
pub mod settings;
pub mod store;
pub mod supplier;
pub mod transfer;
pub mod user;

pub use adjustment::{
    AdjustmentKind, AdjustmentStatus, StockAdjustment, StockAdjustmentItem, StockCount,
    StockCountItem, StockCountStatus,
};
pub use audit::{AuditAction, AuditActor, AuditLog, AuditOutcome};
pub use business::{Business, BusinessStatus};
pub use category::ProductCategory;
pub use common::{
    Address, ContactInfo, CurrencyCode, InvalidCurrencyCode, Money, Quantity, RecordRef,
    RecordStatus, RecordType,
};
pub use customer::Customer;
pub use employee::{Employee, EmployeeStoreRole};
pub use expense::{Expense, ExpenseCategory, ExpensePaymentStatus, ExpenseStatus};
pub use ids::*;
pub use inventory::{InventoryBatch, InventoryItem, StockMovement, StockMovementType};
pub use notification::{Notification, NotificationChannel, NotificationEvent, NotificationStatus};
pub use permission::Permission;
pub use price_history::{PriceHistory, PriceType};
pub use product::{Product, UnitOfMeasure};
pub use product_variant::ProductVariant;
pub use purchase::{
    GoodsReceipt, GoodsReceiptItem, GoodsReceiptStatus, PurchaseOrder, PurchaseOrderItem,
    PurchaseOrderStatus, PurchaseReturn, PurchaseReturnItem, PurchaseReturnStatus,
};
pub use sale::{
    Payment, PaymentMethod, PaymentStatus, Sale, SaleItem, SalePaymentStatus, SaleStatus,
};
pub use sale_return::{
    ItemCondition, Refund, RefundStatus, SaleReturn, SaleReturnItem, SaleReturnStatus,
};
pub use settings::{BusinessSettings, StoreSettings};
pub use store::{Store, StoreStatus};
pub use supplier::{
    Supplier, SupplierLedgerEntry, SupplierLedgerEntryType, SupplierProduct, SupplierStatus,
};
pub use transfer::{StockTransfer, StockTransferItem, TransferStatus};
pub use user::{Role, RoleAssignment, User, UserStatus};
