//! Point-of-sale transactions, line items and payments.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{CurrencyCode, Money, Quantity};
use super::ids::{
    BusinessId, CustomerId, PaymentId, ProductId, ProductVariantId, SaleId, SaleItemId, StoreId,
    UserId,
};

/// A point-of-sale transaction (SRS FR-15, FR-17).
///
/// All amounts are fixed when the sale completes and never recalculated from
/// current prices, so receipts always reproduce the original values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sale {
    pub id: SaleId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub cashier_id: UserId,
    /// `None` for walk-in customers.
    pub customer_id: Option<CustomerId>,
    /// Unique receipt number, e.g. `LAG-000123`.
    pub receipt_number: String,
    pub currency: CurrencyCode,
    /// Sum of line amounts before discounts and taxes.
    pub subtotal: Money,
    pub discount_total: Money,
    pub tax_total: Money,
    /// Amount payable: `subtotal - discount_total + tax_total`.
    pub total: Money,
    /// Sum of successful payments.
    pub amount_paid: Money,
    pub status: SaleStatus,
    pub payment_status: SalePaymentStatus,
    /// Client-supplied key so a retried request cannot create a duplicate
    /// sale (SRS NFR-10).
    pub idempotency_key: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl Sale {
    /// Amount still owed by the customer (never negative).
    pub fn balance_due(&self) -> Money {
        (self.total - self.amount_paid).max(Money::ZERO)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SaleStatus {
    /// Items are being added; no stock has moved yet.
    #[default]
    Draft,
    /// Finalised; stock has been deducted.
    Completed,
    /// Cancelled by an authorised user; any stock deduction was reversed.
    Voided,
}

/// Payment progress of a sale, derived from its [`Payment`] records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SalePaymentStatus {
    #[default]
    Unpaid,
    PartiallyPaid,
    Paid,
    PartiallyRefunded,
    Refunded,
}

/// One line on a sale.
///
/// Product name and SKU are copied at the time of sale so receipts stay
/// correct if the product is renamed later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaleItem {
    pub id: SaleItemId,
    pub sale_id: SaleId,
    pub product_id: ProductId,
    pub variant_id: Option<ProductVariantId>,
    pub product_name: String,
    pub sku: String,
    pub quantity: Quantity,
    /// Selling price per unit at the time of sale (SRS FR-09).
    pub unit_price: Money,
    pub discount_amount: Money,
    pub tax_amount: Money,
    /// `quantity * unit_price - discount_amount + tax_amount`.
    pub line_total: Money,
}

/// A payment recorded against a sale. A sale may have several (split
/// payments) (SRS FR-16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Payment {
    pub id: PaymentId,
    pub sale_id: SaleId,
    pub method: PaymentMethod,
    pub amount: Money,
    pub currency: CurrencyCode,
    pub status: PaymentStatus,
    /// Card terminal or bank reference, where applicable.
    pub external_reference: Option<String>,
    /// Prevents duplicate payments on retried callbacks (SRS NFR-10).
    pub idempotency_key: Option<String>,
    pub received_by: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    Card,
    BankTransfer,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    #[default]
    Pending,
    Successful,
    Failed,
    Reversed,
    Refunded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn money_round_trips_through_json_without_precision_loss() {
        let amount: Money = dec!(1999.99);
        let json = serde_json::to_string(&amount).unwrap();
        assert_eq!(json, "\"1999.99\"");
        assert_eq!(serde_json::from_str::<Money>(&json).unwrap(), amount);
    }
}
