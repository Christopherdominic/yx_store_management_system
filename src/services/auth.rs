//! Authentication and session management (FR-04, NFR-16, NFR-17).

use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    middlewares::auth::AuthUser,
    models::{
        audit::AuditOutcome,
        roles::{BusinessRole, StoreRole},
        user::{Membership, StoreAssignment, User},
    },
    services::audit::AuditEntry,
    state::AppState,
    utils::{
        crypto::{
            generate_token, hash_password, hash_password_async, hash_token, verify_password,
            verify_password_async,
        },
        validation::{Validator, clean, clean_opt, normalize_email},
    },
};

const INVALID_CREDENTIALS: &str = "invalid email or password";

#[derive(Debug, Deserialize)]
pub struct RegisterInput {
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResult {
    pub token: String,
    pub token_type: &'static str,
    pub expires_at: DateTime<Utc>,
    pub user: User,
}

/// Public sign-up for business owners only. The new account has no access
/// to anything until it creates a business (or is added to one).
pub async fn register_owner(state: &AppState, input: RegisterInput) -> AppResult<User> {
    if !state.config.allow_owner_signup {
        return Err(AppError::Forbidden);
    }
    let full_name = clean(input.full_name);
    let email = normalize_email(&input.email);
    let phone = clean_opt(input.phone);

    let mut v = Validator::new();
    v.required("full_name", &full_name, 200);
    v.required("email", &email, 254);
    v.email("email", Some(&email));
    v.optional("phone", phone.as_deref(), 30);
    v.password("password", &input.password);
    v.finish()?;

    let password_hash = hash_password_async(input.password).await?;
    let mut tx = state.pool.begin().await?;
    let user = insert_user(
        &mut *tx,
        &full_name,
        &email,
        phone.as_deref(),
        &password_hash,
        None,
    )
    .await?;
    AuditEntry::new("auth.registered", "user")
        .actor(user.id)
        .entity(user.id)
        .after(&user)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(user)
}

pub async fn insert_user(
    executor: impl PgExecutor<'_>,
    full_name: &str,
    email: &str,
    phone: Option<&str>,
    password_hash: &str,
    created_by: Option<Uuid>,
) -> AppResult<User> {
    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (id, email, full_name, phone, password_hash, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(email)
    .bind(full_name)
    .bind(phone)
    .bind(password_hash)
    .bind(created_by)
    .fetch_one(executor)
    .await?;
    Ok(user)
}

pub async fn get_user(pool: &PgPool, user_id: Uuid) -> AppResult<User> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound("user"))
}

pub async fn login(state: &AppState, email: &str, password: String) -> AppResult<LoginResult> {
    let email = normalize_email(email);
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE lower(email) = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;

    let Some(user) = user else {
        // Spend the same time as a real check so response timing does not
        // reveal which emails are registered.
        let _ = tokio::task::spawn_blocking(move || verify_password(&password, dummy_hash())).await;
        login_failed(state, None, "unknown_email").await;
        return Err(AppError::Unauthorized(INVALID_CREDENTIALS.into()));
    };

    if user.locked_until.is_some_and(|until| until > Utc::now()) {
        login_failed(state, Some(user.id), "locked").await;
        return Err(AppError::TooManyRequests(
            "account temporarily locked after too many failed login attempts; try again later"
                .into(),
        ));
    }

    if !verify_password_async(password, user.password_hash.clone()).await? {
        record_failed_attempt(state, user.id).await?;
        login_failed(state, Some(user.id), "wrong_password").await;
        return Err(AppError::Unauthorized(INVALID_CREDENTIALS.into()));
    }

    if !user.is_active {
        login_failed(state, Some(user.id), "account_deactivated").await;
        return Err(AppError::Unauthorized("account is deactivated".into()));
    }

    let token = generate_token();
    let expires_at = Utc::now() + state.config.session_ttl;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE users SET failed_login_attempts = 0, locked_until = NULL WHERE id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let session_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(session_id)
    .bind(user.id)
    .bind(hash_token(&token))
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new("auth.login", "session")
        .actor(user.id)
        .entity(session_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(LoginResult {
        token,
        token_type: "Bearer",
        expires_at,
        user,
    })
}

fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password("timing-equalizer-1").expect("argon2 hashing works"))
}

/// Counts a failed attempt and locks the account once the limit is reached.
async fn record_failed_attempt(state: &AppState, user_id: Uuid) -> AppResult<()> {
    let attempts: i32 = sqlx::query_scalar(
        "UPDATE users SET failed_login_attempts = failed_login_attempts + 1
         WHERE id = $1 RETURNING failed_login_attempts",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;

    if attempts >= state.config.max_failed_logins {
        let locked_until = Utc::now() + state.config.lockout_duration;
        sqlx::query("UPDATE users SET failed_login_attempts = 0, locked_until = $2 WHERE id = $1")
            .bind(user_id)
            .bind(locked_until)
            .execute(&state.pool)
            .await?;
        AuditEntry::new("auth.locked", "user")
            .actor(user_id)
            .entity(user_id)
            .metadata(json!({ "locked_until": locked_until }))
            .record_best_effort(&state.pool)
            .await;
    }
    Ok(())
}

async fn login_failed(state: &AppState, user_id: Option<Uuid>, reason: &str) {
    let mut entry = AuditEntry::new("auth.login", "session")
        .outcome(AuditOutcome::Failure)
        .metadata(json!({ "reason": reason }));
    if let Some(id) = user_id {
        entry = entry.actor(id).entity(id);
    }
    entry.record_best_effort(&state.pool).await;
}

/// Resolves a bearer token to its user, enforcing expiry, idle timeout,
/// revocation, and account status, and marks the session as used.
pub async fn authenticate(state: &AppState, token: &str) -> AppResult<AuthUser> {
    let idle_cutoff = Utc::now() - state.config.session_idle_timeout;
    sqlx::query_as::<_, AuthUser>(
        "UPDATE sessions s SET last_seen_at = now()
         FROM users u
         WHERE s.token_hash = $1
           AND u.id = s.user_id
           AND s.revoked_at IS NULL
           AND s.expires_at > now()
           AND s.last_seen_at > $2
           AND u.is_active
         RETURNING s.id AS session_id, u.id AS user_id, u.email, u.full_name",
    )
    .bind(hash_token(token))
    .bind(idle_cutoff)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("session is invalid, expired, or revoked".into()))
}

pub async fn logout(state: &AppState, user: &AuthUser) -> AppResult<()> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
        .bind(user.session_id)
        .execute(&mut *tx)
        .await?;
    AuditEntry::new("auth.logout", "session")
        .actor(user.user_id)
        .entity(user.session_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn logout_all(state: &AppState, user: &AuthUser) -> AppResult<u64> {
    let mut tx = state.pool.begin().await?;
    let revoked = revoke_sessions(&mut *tx, user.user_id, None).await?;
    AuditEntry::new("auth.logout_all", "user")
        .actor(user.user_id)
        .entity(user.user_id)
        .metadata(json!({ "sessions_revoked": revoked }))
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(revoked)
}

/// Revokes every active session of a user, optionally keeping one.
pub async fn revoke_sessions(
    executor: impl PgExecutor<'_>,
    user_id: Uuid,
    except_session: Option<Uuid>,
) -> AppResult<u64> {
    let result = sqlx::query(
        "UPDATE sessions SET revoked_at = now()
         WHERE user_id = $1 AND revoked_at IS NULL AND ($2::uuid IS NULL OR id <> $2)",
    )
    .bind(user_id)
    .bind(except_session)
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordInput {
    pub current_password: String,
    pub new_password: String,
}

/// Changes the caller's password and signs out their other sessions.
pub async fn change_password(
    state: &AppState,
    auth: &AuthUser,
    input: ChangePasswordInput,
) -> AppResult<()> {
    let mut v = Validator::new();
    v.password("new_password", &input.new_password);
    v.check(
        input.new_password != input.current_password,
        "new_password",
        "must differ from the current password",
    );
    v.finish()?;

    let user = get_user(&state.pool, auth.user_id).await?;
    if !verify_password_async(input.current_password, user.password_hash).await? {
        return Err(AppError::field("current_password", "is incorrect"));
    }
    let new_hash = hash_password_async(input.new_password).await?;

    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
        .bind(auth.user_id)
        .bind(new_hash)
        .execute(&mut *tx)
        .await?;
    let revoked = revoke_sessions(&mut *tx, auth.user_id, Some(auth.session_id)).await?;
    AuditEntry::new("auth.password_changed", "user")
        .actor(auth.user_id)
        .entity(auth.user_id)
        .metadata(json!({ "other_sessions_revoked": revoked }))
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Creates a single-use reset token for an active account. Returns `None`
/// for unknown or inactive emails; callers must respond identically either
/// way so the endpoint does not reveal which emails exist.
pub async fn request_password_reset(state: &AppState, email: &str) -> AppResult<Option<String>> {
    let email = normalize_email(email);
    let user: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE lower(email) = $1 AND is_active")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    let Some((user_id,)) = user else {
        return Ok(None);
    };

    let token = generate_token();
    let mut tx = state.pool.begin().await?;
    // Only the newest token stays usable.
    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = now() WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(hash_token(&token))
    .bind(Utc::now() + state.config.password_reset_ttl)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new("auth.password_reset_requested", "user")
        .actor(user_id)
        .entity(user_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Some(token))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordInput {
    pub token: String,
    pub new_password: String,
}

/// Consumes a reset token, sets the new password, clears any lockout, and
/// revokes every session of the account.
pub async fn reset_password(state: &AppState, input: ResetPasswordInput) -> AppResult<()> {
    let mut v = Validator::new();
    v.password("new_password", &input.new_password);
    v.finish()?;
    let new_hash = hash_password_async(input.new_password).await?;

    let mut tx = state.pool.begin().await?;
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, user_id FROM password_reset_tokens
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
         FOR UPDATE",
    )
    .bind(hash_token(input.token.trim()))
    .fetch_optional(&mut *tx)
    .await?;
    let Some((token_id, user_id)) = row else {
        return Err(AppError::BadRequest(
            "reset token is invalid or has expired".into(),
        ));
    };

    sqlx::query("UPDATE password_reset_tokens SET used_at = now() WHERE id = $1")
        .bind(token_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE users SET password_hash = $2, failed_login_attempts = 0, locked_until = NULL,
                          updated_at = now()
         WHERE id = $1",
    )
    .bind(user_id)
    .bind(new_hash)
    .execute(&mut *tx)
    .await?;
    revoke_sessions(&mut *tx, user_id, None).await?;
    AuditEntry::new("auth.password_reset", "user")
        .actor(user_id)
        .entity(user_id)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Businesses the user belongs to and their roles in each.
pub async fn memberships(pool: &PgPool, user_id: Uuid) -> AppResult<Vec<Membership>> {
    let businesses: Vec<(Uuid, String, Option<BusinessRole>)> = sqlx::query_as(
        "SELECT b.id, b.trading_name, bm.business_role
         FROM business_members bm
         JOIN businesses b ON b.id = bm.business_id
         WHERE bm.user_id = $1 AND bm.is_active
         ORDER BY b.trading_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let assignments: Vec<(Uuid, Uuid, String, String, StoreRole)> = sqlx::query_as(
        "SELECT s.business_id, sa.store_id, s.name, s.code, sa.role
         FROM store_assignments sa
         JOIN stores s ON s.id = sa.store_id
         WHERE sa.user_id = $1
         ORDER BY s.name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(businesses
        .into_iter()
        .map(|(business_id, business_name, business_role)| Membership {
            business_id,
            business_name,
            business_role,
            stores: assignments
                .iter()
                .filter(|a| a.0 == business_id)
                .map(
                    |(_, store_id, store_name, store_code, role)| StoreAssignment {
                        store_id: *store_id,
                        store_name: store_name.clone(),
                        store_code: store_code.clone(),
                        role: *role,
                    },
                )
                .collect(),
        })
        .collect())
}
