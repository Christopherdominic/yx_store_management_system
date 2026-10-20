//! Price history (SRS FR-09).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::Money;
use super::ids::{BusinessId, PriceHistoryId, ProductId, ProductVariantId, UserId};

/// Append-only record of one price change, written in the same database
/// transaction as the price update.
///
/// Changing both cost and selling price produces two entries. Completed sale
/// items keep their own price and discount snapshots, so price changes never
/// alter historical sales.
///
/// Permissions, approval thresholds, ownership checks and scheduling of
/// future-effective changes belong in the service layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceHistory {
    pub id: PriceHistoryId,
    pub business_id: BusinessId,
    pub product_id: ProductId,
    /// `None` for a change to the parent product's price.
    pub variant_id: Option<ProductVariantId>,
    pub price_type: PriceType,
    pub previous_price: Money,
    pub new_price: Money,
    /// When the new price takes effect.
    pub effective_at: DateTime<Utc>,
    pub changed_by: UserId,
    /// Set when an approval threshold applied.
    pub approved_by: Option<UserId>,
    pub reason: Option<String>,
    /// When the change was recorded.
    pub created_at: DateTime<Utc>,
}

impl PriceHistory {
    /// `new_price - previous_price`; negative for a price cut.
    pub fn difference(&self) -> Money {
        self.new_price - self.previous_price
    }

    /// Percentage change, rounded to 2 decimal places. `None` when the
    /// previous price was zero.
    pub fn percent_change(&self) -> Option<Money> {
        if self.previous_price.is_zero() {
            return None;
        }
        Some((self.difference() / self.previous_price * Money::ONE_HUNDRED).round_dp(2))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceType {
    Cost,
    Selling,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn change(previous: Money, new: Money) -> PriceHistory {
        PriceHistory {
            id: PriceHistoryId::generate(),
            business_id: BusinessId::generate(),
            product_id: ProductId::generate(),
            variant_id: None,
            price_type: PriceType::Selling,
            previous_price: previous,
            new_price: new,
            effective_at: Utc::now(),
            changed_by: UserId::generate(),
            approved_by: None,
            reason: Some("Supplier price increase".into()),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn difference_and_percent_change() {
        let c = change(dec!(1000), dec!(1150));
        assert_eq!(c.difference(), dec!(150));
        assert_eq!(c.percent_change(), Some(dec!(15)));

        let cut = change(dec!(300), dec!(200));
        assert_eq!(cut.percent_change(), Some(dec!(-33.33)));
    }

    #[test]
    fn percent_change_from_zero_is_undefined() {
        assert_eq!(change(dec!(0), dec!(500)).percent_change(), None);
    }
}
