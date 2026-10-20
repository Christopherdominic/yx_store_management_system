use std::env;

use anyhow::Context;
use chrono::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub db_max_connections: u32,
    /// Absolute lifetime of a session, regardless of activity.
    pub session_ttl: Duration,
    /// A session unused for this long is rejected (NFR-17).
    pub session_idle_timeout: Duration,
    pub password_reset_ttl: Duration,
    /// Consecutive failed logins before the account is locked (NFR-16).
    pub max_failed_logins: i32,
    pub lockout_duration: Duration,
    /// Allow anyone to register a business-owner account. Staff accounts are
    /// always created by an administrator (FR-05).
    pub allow_owner_signup: bool,
    /// Development only: return password-reset tokens in the API response
    /// because no email provider is configured yet.
    pub expose_reset_tokens: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            db_max_connections: parse_or("DB_MAX_CONNECTIONS", 10)?,
            session_ttl: Duration::minutes(parse_or("SESSION_TTL_MINUTES", 12 * 60)?),
            session_idle_timeout: Duration::minutes(parse_or("SESSION_IDLE_MINUTES", 15)?),
            password_reset_ttl: Duration::minutes(parse_or("PASSWORD_RESET_TTL_MINUTES", 30)?),
            max_failed_logins: parse_or("MAX_FAILED_LOGINS", 5)?,
            lockout_duration: Duration::minutes(parse_or("LOCKOUT_MINUTES", 15)?),
            allow_owner_signup: parse_or("ALLOW_OWNER_SIGNUP", true)?,
            expose_reset_tokens: parse_or("EXPOSE_RESET_TOKENS", false)?,
        })
    }

    /// Settings used by tests; no environment required.
    pub fn for_tests() -> Self {
        Self {
            database_url: String::new(),
            bind_addr: "127.0.0.1:0".into(),
            db_max_connections: 5,
            session_ttl: Duration::hours(12),
            session_idle_timeout: Duration::minutes(15),
            password_reset_ttl: Duration::minutes(30),
            max_failed_logins: 5,
            lockout_duration: Duration::minutes(15),
            allow_owner_signup: true,
            expose_reset_tokens: true,
        }
    }
}

fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(raw) => raw
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid value for {key}: {e}")),
        Err(_) => Ok(default),
    }
}
