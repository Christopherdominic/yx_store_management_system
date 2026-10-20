//! Businesses (tenants). Every other record belongs to exactly one business.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{Address, ContactInfo, CurrencyCode};
use super::ids::BusinessId;

/// A business profile (SRS FR-01, FR-03).
///
/// Stores are not embedded here; each [`Store`](super::Store) references its
/// business through `business_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Business {
    pub id: BusinessId,
    pub legal_name: String,
    pub trading_name: Option<String>,
    pub registration_number: Option<String>,
    pub address: Address,
    pub contact: ContactInfo,
    pub logo_url: Option<String>,
    /// Default currency for transactions recorded under this business.
    pub base_currency: CurrencyCode,
    /// IANA timezone name, e.g. `Africa/Lagos`.
    pub timezone: String,
    pub status: BusinessStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Business {
    /// Name shown on receipts and in the UI.
    pub fn display_name(&self) -> &str {
        self.trading_name.as_deref().unwrap_or(&self.legal_name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BusinessStatus {
    #[default]
    Active,
    Suspended,
}
