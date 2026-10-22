use std::{collections::BTreeMap, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer};

use crate::{
    errors::{AppError, FieldError},
    models::{Address, CurrencyCode, Money},
    utils::currency::is_valid_currency,
};

/// Largest price accepted; matches the `NUMERIC(14, 2)` columns.
fn max_price() -> Decimal {
    Decimal::new(99_999_999_999_999, 2)
}

pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_PASSWORD_LEN: usize = 128;

/// Collects field errors so a request reports every problem at once.
#[derive(Default)]
pub struct Validator {
    errors: Vec<FieldError>,
}

impl Validator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn error(&mut self, field: &str, message: impl Into<String>) {
        self.errors.push(FieldError {
            field: field.into(),
            message: message.into(),
        });
    }

    pub fn check(&mut self, ok: bool, field: &str, message: &str) {
        if !ok {
            self.error(field, message);
        }
    }

    pub fn required(&mut self, field: &str, value: &str, max_len: usize) {
        if value.trim().is_empty() {
            self.error(field, "is required");
        } else if value.chars().count() > max_len {
            self.error(field, format!("must be at most {max_len} characters"));
        }
    }

    pub fn optional(&mut self, field: &str, value: Option<&str>, max_len: usize) {
        if let Some(v) = value
            && v.chars().count() > max_len
        {
            self.error(field, format!("must be at most {max_len} characters"));
        }
    }

    pub fn email(&mut self, field: &str, value: Option<&str>) {
        if let Some(v) = value
            && !is_plausible_email(v)
        {
            self.error(field, "must be a valid email address");
        }
    }

    pub fn password(&mut self, field: &str, value: &str) {
        let len = value.chars().count();
        if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&len) {
            self.error(
                field,
                format!("must be between {MIN_PASSWORD_LEN} and {MAX_PASSWORD_LEN} characters"),
            );
        } else if !value.chars().any(|c| c.is_alphabetic())
            || !value.chars().any(|c| c.is_ascii_digit())
        {
            self.error(field, "must contain at least one letter and one digit");
        }
    }

    /// Parses an ISO 4217 code; returns `None` (and records an error) when invalid.
    pub fn currency(&mut self, field: &str, value: &str) -> Option<CurrencyCode> {
        match CurrencyCode::new(value) {
            Ok(code) if is_valid_currency(code.as_str()) => Some(code),
            _ => {
                self.error(field, "must be a valid ISO 4217 currency code, e.g. NGN");
                None
            }
        }
    }

    pub fn address(&mut self, field: &str, address: &Address) {
        self.required(&format!("{field}.line1"), &address.line1, 200);
        self.optional(&format!("{field}.line2"), address.line2.as_deref(), 200);
        self.required(&format!("{field}.city"), &address.city, 100);
        self.optional(&format!("{field}.state"), address.state.as_deref(), 100);
        self.optional(
            &format!("{field}.postal_code"),
            address.postal_code.as_deref(),
            20,
        );
        if address.country.len() != 2 || !address.country.bytes().all(|b| b.is_ascii_uppercase()) {
            self.error(
                &format!("{field}.country"),
                "must be an ISO 3166-1 alpha-2 code, e.g. NG",
            );
        }
    }

    pub fn timezone(&mut self, field: &str, value: &str) {
        if chrono_tz::Tz::from_str(value).is_err() {
            self.error(field, "must be a valid IANA timezone, e.g. Africa/Lagos");
        }
    }

    /// Short uppercase identifiers such as store codes and receipt prefixes.
    pub fn code(&mut self, field: &str, value: &str, max_len: usize) {
        let valid = !value.is_empty()
            && value.len() <= max_len
            && value
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-');
        if !valid {
            self.error(
                field,
                format!("must be 1-{max_len} characters of A-Z, 0-9 or '-'"),
            );
        }
    }

    /// Product and variant SKUs: 1-64 characters of A-Z, 0-9, '-', '_' or '.'.
    pub fn sku(&mut self, field: &str, value: &str) {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "-_.".contains(c));
        if !valid {
            self.error(
                field,
                "must be 1-64 characters of A-Z, 0-9, '-', '_' or '.'",
            );
        }
    }

    /// Barcodes (EAN, UPC, Code 128, ...): 1-64 letters, digits or '-'.
    pub fn barcode(&mut self, field: &str, value: Option<&str>) {
        if let Some(v) = value
            && (v.len() > 64 || !v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        {
            self.error(field, "must be 1-64 letters, digits or '-'");
        }
    }

    /// Money in the business currency: not negative, at most 2 decimal places.
    pub fn price(&mut self, field: &str, value: Money) {
        if value.is_sign_negative() && !value.is_zero() {
            self.error(field, "must not be negative");
        } else if value.normalize().scale() > 2 {
            self.error(field, "must have at most 2 decimal places");
        } else if value > max_price() {
            self.error(field, "is too large");
        }
    }

    pub fn url(&mut self, field: &str, value: Option<&str>) {
        if let Some(v) = value
            && (v.len() > 2000
                || !(v.starts_with("https://") || v.starts_with("http://"))
                || v.chars().any(char::is_whitespace))
        {
            self.error(field, "must be an http(s) URL of at most 2000 characters");
        }
    }

    /// Free-form key/value attributes such as size or colour.
    pub fn attributes(&mut self, field: &str, attributes: &BTreeMap<String, String>) {
        if attributes.len() > 30 {
            self.error(field, "must have at most 30 entries");
        }
        for (key, value) in attributes {
            if key.is_empty() || key.chars().count() > 50 {
                self.error(field, "keys must be 1-50 characters");
                break;
            }
            if value.is_empty() || value.chars().count() > 200 {
                self.error(&format!("{field}.{key}"), "must be 1-200 characters");
            }
        }
    }

    pub fn finish(self) -> Result<(), AppError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(self.errors))
        }
    }
}

fn is_plausible_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !value.chars().any(char::is_whitespace)
        && value.len() <= 254
}

/// Trims a required string.
pub fn clean(value: String) -> String {
    value.trim().to_string()
}

/// Trims an optional string; blank becomes `None`.
pub fn clean_opt(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// PATCH semantics for nullable text: absent keeps the current value, an
/// empty string clears it, anything else replaces it.
pub fn patch_opt(current: Option<String>, incoming: Option<String>) -> Option<String> {
    match incoming {
        None => current,
        Some(v) => clean_opt(Some(v)),
    }
}

/// Trims attribute keys and values.
pub fn clean_attributes(attributes: BTreeMap<String, String>) -> BTreeMap<String, String> {
    attributes
        .into_iter()
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// For PATCH fields that can be cleared with `null`: absent → `None`,
/// `null` → `Some(None)`, a value → `Some(Some(value))`. Use with
/// `#[serde(default, deserialize_with = "nullable")]`.
pub fn nullable<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Escapes `%`, `_` and `\` so user input is matched literally in ILIKE.
pub fn like_pattern(term: &str) -> String {
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Trims every part of an address and uppercases the country code.
pub fn clean_address(address: Address) -> Address {
    Address {
        line1: clean(address.line1),
        line2: clean_opt(address.line2),
        city: clean(address.city),
        state: clean_opt(address.state),
        postal_code: clean_opt(address.postal_code),
        country: address.country.trim().to_ascii_uppercase(),
    }
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn price_errors(value: Money) -> usize {
        let mut v = Validator::new();
        v.price("price", value);
        v.errors.len()
    }

    #[test]
    fn price_limits() {
        assert_eq!(max_price(), dec!(999_999_999_999.99));
        assert_eq!(price_errors(dec!(0)), 0);
        assert_eq!(price_errors(dec!(72500.50)), 0);
        assert_eq!(price_errors(dec!(1.500)), 0, "trailing zeros are fine");
        assert_eq!(price_errors(dec!(-0.01)), 1);
        assert_eq!(price_errors(dec!(1.005)), 1);
        assert_eq!(price_errors(dec!(1_000_000_000_000)), 1);
    }

    #[test]
    fn sku_and_barcode_rules() {
        let mut v = Validator::new();
        v.sku("sku", "RICE-50KG_1.0");
        v.barcode("barcode", Some("6151234567890"));
        assert!(v.errors.is_empty());
        v.sku("sku", "rice 50");
        v.barcode("barcode", Some("12 34"));
        assert_eq!(v.errors.len(), 2);
    }
}
