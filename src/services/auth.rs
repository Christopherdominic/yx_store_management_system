//! Authentication and session management (FR-04, NFR-16, NFR-17).

use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    middlewares::auth::AuthUser,
    models::{
        AuditAction, AuditOutcome, Business, BusinessId, RecordType, Role, RoleAssignmentId,
        StoreId, User, UserId,
    },
    services::{
        audit::AuditEntry,
        businesses::{self, CreateBusiness},
    },
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
pub struct NewUser {
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub password: String,
}

impl NewUser {
    pub fn clean(self) -> Self {
        Self {
            full_name: clean(self.full_name),
            email: normalize_email(&self.email),
            phone: clean_opt(self.phone),
            password: self.password,
        }
    }

    pub fn validate(&self, v: &mut Validator, prefix: &str) {
        v.required(&format!("{prefix}full_name"), &self.full_name, 200);
        v.required(&format!("{prefix}email"), &self.email, 254);
        v.email(&format!("{prefix}email"), Some(&self.email));
        v.optional(&format!("{prefix}phone"), self.phone.as_deref(), 30);
        v.password(&format!("{prefix}password"), &self.password);
    }
}

/// Self-service onboarding: creates a business and its owner account.
/// Staff accounts are created by administrators (FR-05).
#[derive(Debug, Deserialize)]
pub struct RegisterInput {
    pub business: CreateBusiness,
    pub owner: NewUser,
}

#[derive(Debug, Serialize)]
pub struct Registration {
    pub business: Business,
    pub owner: User,
}

pub async fn register(state: &AppState, input: RegisterInput) -> AppResult<Registration> {
    if !state.config.allow_owner_signup {
        return Err(AppError::Forbidden);
    }
    let owner = input.owner.clean();
    let mut v = Validator::new();
    let business = businesses::prepare(input.business, &mut v);
    owner.validate(&mut v, "owner.");
    v.finish()?;

    let password_hash = hash_password_async(owner.password.clone()).await?;
    let mut tx = state.pool.begin().await?;
    let business = businesses::insert(&mut tx, &business).await?;
    let user = insert_user(&mut tx, business.id, &owner, &password_hash).await?;
    insert_role(
        &mut tx,
        business.id,
        user.id,
        None,
        Role::BusinessOwner,
        user.id,
    )
    .await?;
    AuditEntry::new(business.id, AuditAction::Created)
        .actor(user.id)
        .record(RecordType::Business, business.id)
        .after(&business)
        .record_in(&mut *tx)
        .await?;
    AuditEntry::new(business.id, AuditAction::Created)
        .actor(user.id)
        .record(RecordType::User, user.id)
        .after(&user)
        .details("business owner registered")
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Registration {
        business,
        owner: user,
    })
}

/// Inserts a user and their credentials (`user.password` is ignored; pass
/// its hash).
pub async fn insert_user(
    conn: &mut PgConnection,
    business_id: BusinessId,
    user: &NewUser,
    password_hash: &str,
) -> AppResult<User> {
    let created = sqlx::query_as::<_, User>(
        "INSERT INTO users (id, business_id, full_name, email, phone)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING *",
    )
    .bind(UserId::generate())
    .bind(business_id)
    .bind(&user.full_name)
    .bind(&user.email)
    .bind(&user.phone)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO user_credentials (user_id, password_hash) VALUES ($1, $2)")
        .bind(created.id)
        .bind(password_hash)
        .execute(&mut *conn)
        .await?;
    Ok(created)
}

pub async fn insert_role(
    conn: &mut PgConnection,
    business_id: BusinessId,
    user_id: UserId,
    store_id: Option<StoreId>,
    role: Role,
    assigned_by: UserId,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO role_assignments (id, business_id, user_id, store_id, role, assigned_by)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(RoleAssignmentId::generate())
    .bind(business_id)
    .bind(user_id)
    .bind(store_id)
    .bind(role)
    .bind(assigned_by)
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct LoginResult {
    pub token: String,
    pub token_type: &'static str,
    pub expires_at: DateTime<Utc>,
    pub user: User,
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
        tracing::info!("login failed: unknown email");
        return Err(AppError::Unauthorized(INVALID_CREDENTIALS.into()));
    };

    let (password_hash, locked_until): (String, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT password_hash, locked_until FROM user_credentials WHERE user_id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    if locked_until.is_some_and(|until| until > Utc::now()) {
        login_failed(state, &user, "account locked").await;
        return Err(AppError::TooManyRequests(
            "account temporarily locked after too many failed login attempts; try again later"
                .into(),
        ));
    }

    if !verify_password_async(password, password_hash).await? {
        record_failed_attempt(state, &user).await?;
        login_failed(state, &user, "wrong password").await;
        return Err(AppError::Unauthorized(INVALID_CREDENTIALS.into()));
    }

    if !user.is_active() {
        login_failed(state, &user, "account deactivated").await;
        return Err(AppError::Unauthorized("account is deactivated".into()));
    }

    let token = generate_token();
    let expires_at = Utc::now() + state.config.session_ttl;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE user_credentials SET failed_login_attempts = 0, locked_until = NULL
         WHERE user_id = $1",
    )
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    let user = sqlx::query_as::<_, User>(
        "UPDATE users SET last_login_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(user.id)
    .bind(hash_token(&token))
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new(user.business_id, AuditAction::Login)
        .actor(user.id)
        .record(RecordType::User, user.id)
        .record_in(&mut *tx)
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
async fn record_failed_attempt(state: &AppState, user: &User) -> AppResult<()> {
    let attempts: i32 = sqlx::query_scalar(
        "UPDATE user_credentials SET failed_login_attempts = failed_login_attempts + 1
         WHERE user_id = $1 RETURNING failed_login_attempts",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    if attempts >= state.config.max_failed_logins {
        let locked_until = Utc::now() + state.config.lockout_duration;
        sqlx::query(
            "UPDATE user_credentials SET failed_login_attempts = 0, locked_until = $2
             WHERE user_id = $1",
        )
        .bind(user.id)
        .bind(locked_until)
        .execute(&state.pool)
        .await?;
        login_failed(state, user, &format!("account locked until {locked_until}")).await;
    }
    Ok(())
}

async fn login_failed(state: &AppState, user: &User, reason: &str) {
    AuditEntry::new(user.business_id, AuditAction::LoginFailed)
        .actor(user.id)
        .record(RecordType::User, user.id)
        .outcome(AuditOutcome::Failure)
        .details(reason)
        .record_best_effort(&state.pool)
        .await;
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
           AND u.status = 'active'
         RETURNING s.id AS session_id, u.id AS user_id, u.business_id, u.email, u.full_name",
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
    AuditEntry::new(user.business_id, AuditAction::Logout)
        .actor(user.user_id)
        .record(RecordType::User, user.user_id)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn logout_all(state: &AppState, user: &AuthUser) -> AppResult<u64> {
    let mut tx = state.pool.begin().await?;
    let revoked = revoke_sessions(&mut *tx, user.user_id, None).await?;
    AuditEntry::new(user.business_id, AuditAction::Logout)
        .actor(user.user_id)
        .record(RecordType::User, user.user_id)
        .details(format!("all sessions revoked ({revoked})"))
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(revoked)
}

/// Revokes every active session of a user, optionally keeping one.
pub async fn revoke_sessions(
    executor: impl PgExecutor<'_>,
    user_id: UserId,
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

    let current_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM user_credentials WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.pool)
            .await?;
    if !verify_password_async(input.current_password, current_hash).await? {
        return Err(AppError::field("current_password", "is incorrect"));
    }
    let new_hash = hash_password_async(input.new_password).await?;

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE user_credentials SET password_hash = $2, updated_at = now() WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .bind(new_hash)
    .execute(&mut *tx)
    .await?;
    let revoked = revoke_sessions(&mut *tx, auth.user_id, Some(auth.session_id)).await?;
    AuditEntry::new(auth.business_id, AuditAction::PasswordChanged)
        .actor(auth.user_id)
        .record(RecordType::User, auth.user_id)
        .details(format!("other sessions revoked: {revoked}"))
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Creates a single-use reset token for an active account. Returns `None`
/// for unknown or inactive emails; callers must respond identically either
/// way so the endpoint does not reveal which emails exist.
pub async fn request_password_reset(state: &AppState, email: &str) -> AppResult<Option<String>> {
    let email = normalize_email(email);
    let user: Option<(UserId,)> =
        sqlx::query_as("SELECT id FROM users WHERE lower(email) = $1 AND status = 'active'")
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
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(hash_token(&token))
    .bind(Utc::now() + state.config.password_reset_ttl)
    .execute(&mut *tx)
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
    let row: Option<(Uuid, UserId, BusinessId)> = sqlx::query_as(
        "SELECT t.id, t.user_id, u.business_id
         FROM password_reset_tokens t
         JOIN users u ON u.id = t.user_id
         WHERE t.token_hash = $1 AND t.used_at IS NULL AND t.expires_at > now()
         FOR UPDATE OF t",
    )
    .bind(hash_token(input.token.trim()))
    .fetch_optional(&mut *tx)
    .await?;
    let Some((token_id, user_id, business_id)) = row else {
        return Err(AppError::BadRequest(
            "reset token is invalid or has expired".into(),
        ));
    };

    sqlx::query("UPDATE password_reset_tokens SET used_at = now() WHERE id = $1")
        .bind(token_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE user_credentials
         SET password_hash = $2, failed_login_attempts = 0, locked_until = NULL, updated_at = now()
         WHERE user_id = $1",
    )
    .bind(user_id)
    .bind(new_hash)
    .execute(&mut *tx)
    .await?;
    revoke_sessions(&mut *tx, user_id, None).await?;
    AuditEntry::new(business_id, AuditAction::PasswordChanged)
        .actor(user_id)
        .record(RecordType::User, user_id)
        .details("password reset via recovery token")
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
