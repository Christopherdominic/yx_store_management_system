//! Product categories (SRS FR-08).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::RecordStatus;
use super::ids::{BusinessId, ProductCategoryId};

/// Business-owned product category. Products reference it by ID.
///
/// Categories can be nested through `parent_id`, e.g. "Groceries" >
/// "Grains". Archived categories keep their products readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductCategory {
    pub id: ProductCategoryId,
    pub business_id: BusinessId,
    pub parent_id: Option<ProductCategoryId>,
    /// Unique among sibling categories within the business.
    pub name: String,
    pub description: Option<String>,
    pub status: RecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProductCategory {
    pub fn is_active(&self) -> bool {
        self.status == RecordStatus::Active
    }

    pub fn is_top_level(&self) -> bool {
        self.parent_id.is_none()
    }
}
