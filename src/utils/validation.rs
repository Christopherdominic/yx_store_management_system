use std::str::FromStr;

use crate::{
    errors::{AppError, FieldError},
    models::{Address, CurrencyCode},
    utils::currency::is_valid_currency,
};

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
