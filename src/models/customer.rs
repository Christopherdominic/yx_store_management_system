//! Customer records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{Address, ContactInfo};
use super::ids::{BusinessId, CustomerId};

/// A customer known to the business (SRS FR-19).
///
/// Walk-in sales do not need a customer: [`Sale::customer_id`](super::Sale)
/// is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Customer {
    pub id: CustomerId,
    pub business_id: BusinessId,
    pub full_name: String,
    pub contact: ContactInfo,
    pub address: Option<Address>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
