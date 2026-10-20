use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::product::ProductStatus;

/// Business-owned product category (SRS FR-08). Products reference it by ID.
///
/// Categories can be nested one under another through `parent_id`, e.g.
/// "Groceries" > "Grains". Archived categories keep their products readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: u64,
    pub business_id: u64,
    pub parent_id: Option<u64>,
    /// Unique among sibling categories within the business.
    pub name: String,
    pub description: Option<String>,
    pub status: ProductStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Category {
    pub fn is_active(&self) -> bool {
        self.status == ProductStatus::Active
    }

    pub fn is_top_level(&self) -> bool {
        self.parent_id.is_none()
    }
}
