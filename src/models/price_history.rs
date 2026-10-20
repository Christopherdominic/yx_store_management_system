use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Append-only record of one price change (SRS FR-09), written in the same
/// database transaction as the price update.
///
/// Changing both cost and selling price produces two entries. Completed sale
/// items keep their own price and discount snapshots, so price changes never
/// alter historical sales.
///
/// Permissions, approval thresholds, ownership checks and scheduling of
/// future-effective changes belong in the service layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceHistory {
    pub id: u64,
    pub business_id: u64,
    pub product_id: u64,
    /// `None` for a change to the parent product's price.
    pub variant_id: Option<u64>,
    pub price_type: PriceType,
    #[serde(with = "rust_decimal::serde::str")]
    pub previous_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub new_price: Decimal,
    /// When the new price takes effect.
    pub effective_at: DateTime<Utc>,
    /// User who made the change.
    pub changed_by: u64,
    /// User who approved the change, when an approval threshold applied.
    pub approved_by: Option<u64>,
    pub reason: Option<String>,
    /// When the change was recorded.
    pub created_at: DateTime<Utc>,
}

impl PriceHistory {
    /// `new_price - previous_price`; negative for a price cut.
    pub fn difference(&self) -> Decimal {
        self.new_price - self.previous_price
    }

    /// Percentage change, rounded to 2 decimal places. `None` when the
    /// previous price was zero.
    pub fn percent_change(&self) -> Option<Decimal> {
        if self.previous_price.is_zero() {
            return None;
        }
        Some((self.difference() / self.previous_price * Decimal::ONE_HUNDRED).round_dp(2))
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

    fn change(previous: Decimal, new: Decimal) -> PriceHistory {
        PriceHistory {
            id: 1,
            business_id: 1,
            product_id: 1,
            variant_id: None,
            price_type: PriceType::Selling,
            previous_price: previous,
            new_price: new,
            effective_at: Utc::now(),
            changed_by: 7,
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
