//! Business- and store-level settings (SRS FR-03).
//!
//! Settings apply to transactions created after a change. Transactions keep
//! the values they were recorded with, so a change never rewrites history.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::FromRow;

use super::common::CurrencyCode;
use super::ids::{BusinessId, StoreId};

#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct BusinessSettings {
    pub business_id: BusinessId,
    /// Mirrors [`Business::base_currency`](super::Business::base_currency).
    pub base_currency: CurrencyCode,
    /// Mirrors [`Business::timezone`](super::Business::timezone).
    pub timezone: String,
    pub tax_name: String,
    /// Percentage, e.g. `"7.5"` for 7.5% VAT.
    pub tax_rate: Decimal,
    pub prices_include_tax: bool,
    /// Tax identification number printed on receipts.
    pub tax_id: Option<String>,
    pub receipt_header: Option<String>,
    pub receipt_footer: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, FromRow)]
pub struct StoreSettings {
    pub store_id: StoreId,
    pub receipt_prefix: String,
    pub transaction_prefix: String,
    /// Overrides the business receipt footer for this store.
    pub receipt_footer: Option<String>,
    pub updated_at: DateTime<Utc>,
}
