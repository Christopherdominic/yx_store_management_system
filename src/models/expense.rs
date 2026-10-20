//! Store expenses and their configurable categories (SRS FR-24).

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::common::{CurrencyCode, Money, RecordStatus};
use super::ids::{BusinessId, ExpenseCategoryId, ExpenseId, StoreId, UserId};

/// An expense category configured by the business.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpenseCategory {
    pub id: ExpenseCategoryId,
    pub business_id: BusinessId,
    pub name: String,
    pub description: Option<String>,
    pub status: RecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ExpenseCategory {
    /// Categories created for every new business.
    pub const DEFAULTS: &[&str] = &[
        "Electricity",
        "Rent",
        "Transport",
        "Maintenance",
        "Salary",
        "Other",
    ];
}

/// An expense incurred by a store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expense {
    pub id: ExpenseId,
    pub business_id: BusinessId,
    pub store_id: StoreId,
    pub category_id: ExpenseCategoryId,
    pub description: String,
    pub amount: Money,
    pub currency: CurrencyCode,
    /// Date the expense was incurred, in the business's local timezone.
    pub expense_date: NaiveDate,
    pub status: ExpenseStatus,
    pub payment_status: ExpensePaymentStatus,
    /// Invoice or receipt number from the vendor.
    pub reference: Option<String>,
    pub attachment_url: Option<String>,
    pub recorded_by: UserId,
    pub approved_by: Option<UserId>,
    pub approved_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Expense {
    /// Only approved expenses count towards expense totals and reports.
    pub fn counts_toward_totals(&self) -> bool {
        self.status == ExpenseStatus::Approved
    }

    /// Expenses can be edited only before approval.
    pub fn is_editable(&self) -> bool {
        matches!(self.status, ExpenseStatus::Draft | ExpenseStatus::Submitted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseStatus {
    #[default]
    Draft,
    Submitted,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExpensePaymentStatus {
    #[default]
    Unpaid,
    Paid,
}
