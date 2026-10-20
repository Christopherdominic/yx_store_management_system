//! Physical store locations belonging to a business.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{Address, ContactInfo};
use super::ids::{BusinessId, StoreId};

/// A store under a business (SRS FR-02, FR-03).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub id: StoreId,
    pub business_id: BusinessId,
    /// Short code unique within the business, e.g. `LAG`. Used as the prefix
    /// for receipt and transfer numbers.
    pub code: String,
    pub name: String,
    pub address: Address,
    pub contact: ContactInfo,
    pub status: StoreStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Store {
    /// Whether the store may accept new sales, stock movements, purchases
    /// or transfers. Inactive stores keep their history readable.
    pub fn is_operational(&self) -> bool {
        self.status == StoreStatus::Active
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StoreStatus {
    #[default]
    Active,
    Inactive,
}
