use std::str::FromStr;

use crate::{
    errors::{AppError, FieldError},
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

    pub fn currency(&mut self, field: &str, value: &str) {
        if !is_valid_currency(value) {
            self.error(field, "must be a valid ISO 4217 currency code, e.g. NGN");
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

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}
