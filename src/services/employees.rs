//! Employee accounts and role assignments (FR-05).
//!
//! Roles are [`RoleAssignment`](crate::models::RoleAssignment) rows: a
//! business-wide role has no store, a store role names one. Changing a role
//! revokes the old assignment and inserts a new one, so history is kept.
//!
//! Rules beyond the permission table:
//! - Only a business owner may grant/revoke the owner or administrator role,
//!   or modify an employee who holds one.
//! - Nobody can deactivate themselves, and a business always keeps at least
//!   one active owner.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::json;
use sqlx::{PgConnection, PgPool};

use crate::{
    errors::{AppError, AppResult},
    models::{
        AuditAction, BusinessId, Employee, EmployeeStoreRole, Permission, RecordType, Role,
        StoreId, User, UserId, UserStatus,
    },
    services::{
        access::Access,
        audit::AuditEntry,
        auth::{self, NewUser},
    },
    utils::{
        crypto::hash_password_async,
        validation::{Validator, clean, like_pattern, patch_opt},
    },
};

#[derive(Debug, Deserialize)]
pub struct StoreRoleInput {
    pub store_id: StoreId,
    pub role: Role,
}

#[derive(Debug, Deserialize)]
pub struct CreateEmployee {
    #[serde(flatten)]
    pub user: NewUser,
    pub business_role: Option<Role>,
    #[serde(default)]
    pub stores: Vec<StoreRoleInput>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateEmployee {
    pub full_name: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetBusinessRole {
    /// `null` removes the business-wide role.
    pub role: Option<Role>,
}

#[derive(Debug, Deserialize)]
pub struct AssignStore {
    pub role: Role,
}

#[derive(Debug, Default, Deserialize)]
pub struct EmployeeFilter {
    /// Matches name or email (partial).
    pub q: Option<String>,
    pub store_id: Option<StoreId>,
    pub status: Option<UserStatus>,
}

async fn load_user(
    conn: &mut PgConnection,
    business_id: BusinessId,
    user_id: UserId,
    for_update: bool,
) -> AppResult<User> {
    let query = if for_update {
        "SELECT * FROM users WHERE id = $1 AND business_id = $2 FOR UPDATE"
    } else {
        "SELECT * FROM users WHERE id = $1 AND business_id = $2"
    };
    sqlx::query_as::<_, User>(query)
        .bind(user_id)
        .bind(business_id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound("employee"))
}

async fn with_roles(
    conn: &mut PgConnection,
    business_id: BusinessId,
    users: Vec<User>,
) -> AppResult<Vec<Employee>> {
    let ids: Vec<UserId> = users.iter().map(|u| u.id).collect();
    let business_roles: Vec<(UserId, Role)> = sqlx::query_as(
        "SELECT user_id, role FROM role_assignments
         WHERE business_id = $1 AND user_id = ANY($2) AND store_id IS NULL AND revoked_at IS NULL",
    )
    .bind(business_id)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let store_roles: Vec<(
        UserId,
        StoreId,
        String,
        String,
        Role,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        "SELECT ra.user_id, ra.store_id, s.name, s.code, ra.role, ra.assigned_at
             FROM role_assignments ra
             JOIN stores s ON s.id = ra.store_id
             WHERE ra.business_id = $1 AND ra.user_id = ANY($2) AND ra.revoked_at IS NULL
             ORDER BY s.name",
    )
    .bind(business_id)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;

    Ok(users
        .into_iter()
        .map(|user| Employee {
            business_role: business_roles
                .iter()
                .find(|(id, _)| *id == user.id)
                .map(|(_, role)| *role),
            stores: store_roles
                .iter()
                .filter(|r| r.0 == user.id)
                .map(
                    |(_, store_id, name, code, role, assigned_at)| EmployeeStoreRole {
                        store_id: *store_id,
                        store_name: name.clone(),
                        store_code: code.clone(),
                        role: *role,
                        assigned_at: *assigned_at,
                    },
                )
                .collect(),
            user,
        })
        .collect())
}

pub async fn load(
    conn: &mut PgConnection,
    business_id: BusinessId,
    user_id: UserId,
) -> AppResult<Employee> {
    let user = load_user(conn, business_id, user_id, false).await?;
    Ok(with_roles(conn, business_id, vec![user]).await?.remove(0))
}

async fn active_business_role(conn: &mut PgConnection, user_id: UserId) -> AppResult<Option<Role>> {
    Ok(sqlx::query_scalar(
        "SELECT role FROM role_assignments
         WHERE user_id = $1 AND store_id IS NULL AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?)
}

/// Holders of a business-wide role can only be changed by an owner.
async fn ensure_can_modify(
    conn: &mut PgConnection,
    access: &Access,
    target: UserId,
) -> AppResult<()> {
    if active_business_role(conn, target).await?.is_some()
        && !access.can(Permission::BusinessRoleManage, None)
    {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn ensure_stores_in_business(
    conn: &mut PgConnection,
    business_id: BusinessId,
    store_ids: &[StoreId],
) -> AppResult<()> {
    let found: i64 =
        sqlx::query_scalar("SELECT count(*) FROM stores WHERE business_id = $1 AND id = ANY($2)")
            .bind(business_id)
            .bind(store_ids)
            .fetch_one(conn)
            .await?;
    if found as usize != store_ids.len() {
        return Err(AppError::field(
            "stores",
            "contains a store that does not belong to this business",
        ));
    }
    Ok(())
}

/// Locks the active owners of a business and errors if `user_id` is the
/// only one.
async fn ensure_other_owner(
    conn: &mut PgConnection,
    business_id: BusinessId,
    user_id: UserId,
) -> AppResult<()> {
    let owners: Vec<UserId> = sqlx::query_scalar(
        "SELECT ra.user_id FROM role_assignments ra
         JOIN users u ON u.id = ra.user_id
         WHERE ra.business_id = $1 AND ra.role = 'business_owner' AND ra.revoked_at IS NULL
           AND u.status = 'active'
         FOR UPDATE OF ra",
    )
    .bind(business_id)
    .fetch_all(conn)
    .await?;
    if !owners.iter().any(|id| *id != user_id) {
        return Err(AppError::Conflict(
            "a business must keep at least one active owner".into(),
        ));
    }
    Ok(())
}

pub async fn create(pool: &PgPool, access: &Access, input: CreateEmployee) -> AppResult<Employee> {
    if input.business_role.is_some() && !access.can(Permission::BusinessRoleManage, None) {
        return Err(AppError::Forbidden);
    }
    let user = input.user.clean();
    let store_ids: Vec<StoreId> = input.stores.iter().map(|s| s.store_id).collect();

    let mut v = Validator::new();
    user.validate(&mut v, "");
    v.check(
        input.business_role.is_none_or(Role::is_business_wide),
        "business_role",
        "must be business_owner or administrator",
    );
    v.check(
        input.stores.iter().all(|s| !s.role.is_business_wide()),
        "stores",
        "store roles must be store_manager, cashier, inventory_officer, or finance_officer",
    );
    v.check(
        store_ids.iter().collect::<HashSet<_>>().len() == store_ids.len(),
        "stores",
        "lists the same store more than once",
    );
    v.finish()?;

    let password_hash = hash_password_async(user.password.clone()).await?;
    let business_id = access.business_id;
    let actor = access.user_id;

    let mut tx = pool.begin().await?;
    ensure_stores_in_business(&mut tx, business_id, &store_ids).await?;
    let created = auth::insert_user(&mut tx, business_id, &user, &password_hash).await?;
    if let Some(role) = input.business_role {
        auth::insert_role(&mut tx, business_id, created.id, None, role, actor).await?;
    }
    for assignment in &input.stores {
        auth::insert_role(
            &mut tx,
            business_id,
            created.id,
            Some(assignment.store_id),
            assignment.role,
            actor,
        )
        .await?;
    }
    let employee = load(&mut tx, business_id, created.id).await?;
    AuditEntry::new(business_id, AuditAction::Created)
        .actor(actor)
        .record(RecordType::User, created.id)
        .after(&employee)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(employee)
}

/// Lists employees. Without a business-wide role the caller only sees
/// employees assigned to stores where they hold `EmployeeView`.
pub async fn list(
    pool: &PgPool,
    access: &Access,
    filter: EmployeeFilter,
) -> AppResult<Vec<Employee>> {
    let visible = access.stores_with(Permission::EmployeeView);
    let term = filter
        .q
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty());
    let mut conn = pool.acquire().await?;
    let users = sqlx::query_as::<_, User>(
        "SELECT * FROM users u
         WHERE u.business_id = $1
           AND ($2::uuid[] IS NULL OR EXISTS (
                SELECT 1 FROM role_assignments ra
                WHERE ra.user_id = u.id AND ra.revoked_at IS NULL AND ra.store_id = ANY($2)))
           AND ($3::text IS NULL OR u.full_name ILIKE $3 OR u.email ILIKE $3)
           AND ($4::uuid IS NULL OR EXISTS (
                SELECT 1 FROM role_assignments ra
                WHERE ra.user_id = u.id AND ra.revoked_at IS NULL AND ra.store_id = $4))
           AND ($5::text IS NULL OR u.status = $5)
         ORDER BY u.full_name",
    )
    .bind(access.business_id)
    .bind(visible)
    .bind(term.as_deref().map(like_pattern))
    .bind(filter.store_id)
    .bind(filter.status)
    .fetch_all(&mut *conn)
    .await?;
    with_roles(&mut conn, access.business_id, users).await
}

pub async fn get(pool: &PgPool, access: &Access, user_id: UserId) -> AppResult<Employee> {
    let mut conn = pool.acquire().await?;
    let employee = load(&mut conn, access.business_id, user_id).await?;
    if let Some(visible) = access.stores_with(Permission::EmployeeView)
        && user_id != access.user_id
        && !employee
            .stores
            .iter()
            .any(|s| visible.contains(&s.store_id))
    {
        return Err(AppError::Forbidden);
    }
    Ok(employee)
}

pub async fn update(
    pool: &PgPool,
    access: &Access,
    user_id: UserId,
    input: UpdateEmployee,
) -> AppResult<Employee> {
    let mut tx = pool.begin().await?;
    let target = load_user(&mut tx, access.business_id, user_id, true).await?;
    ensure_can_modify(&mut tx, access, user_id).await?;
    let before = load(&mut tx, access.business_id, user_id).await?;

    let full_name = input.full_name.map(clean).unwrap_or(target.full_name);
    let phone = patch_opt(target.phone, input.phone);
    let mut v = Validator::new();
    v.required("full_name", &full_name, 200);
    v.optional("phone", phone.as_deref(), 30);
    v.finish()?;

    sqlx::query("UPDATE users SET full_name = $2, phone = $3, updated_at = now() WHERE id = $1")
        .bind(user_id)
        .bind(&full_name)
        .bind(&phone)
        .execute(&mut *tx)
        .await?;
    let after = load(&mut tx, access.business_id, user_id).await?;
    AuditEntry::new(access.business_id, AuditAction::Updated)
        .actor(access.user_id)
        .record(RecordType::User, user_id)
        .before(&before)
        .after(&after)
        .record_in(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

/// Deactivating also signs the employee out everywhere.
pub async fn set_status(
    pool: &PgPool,
    access: &Access,
    user_id: UserId,
    status: UserStatus,
) -> AppResult<Employee> {
    if status == UserStatus::Deactivated && user_id == access.user_id {
        return Err(AppError::Conflict(
            "you cannot deactivate your own account".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let target = load_user(&mut tx, access.business_id, user_id, true).await?;
    ensure_can_modify(&mut tx, access, user_id).await?;
    if target.status == status {
        return load(&mut tx, access.business_id, user_id).await;
    }
    if status == UserStatus::Deactivated
        && active_business_role(&mut tx, user_id).await? == Some(Role::BusinessOwner)
    {
        ensure_other_owner(&mut tx, access.business_id, user_id).await?;
    }

    sqlx::query("UPDATE users SET status = $2, updated_at = now() WHERE id = $1")
        .bind(user_id)
        .bind(status)
        .execute(&mut *tx)
        .await?;
    let (action, revoked) = match status {
        UserStatus::Active => (AuditAction::Reactivated, 0),
        UserStatus::Deactivated => (
            AuditAction::Deactivated,
            auth::revoke_sessions(&mut *tx, user_id, None).await?,
        ),
    };
    AuditEntry::new(access.business_id, action)
        .actor(access.user_id)
        .record(RecordType::User, user_id)
        .before(&json!({ "status": target.status }))
        .after(&json!({ "status": status }))
        .details(format!("sessions revoked: {revoked}"))
        .record_in(&mut *tx)
        .await?;
    let updated = load(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}

/// Grants, changes, or removes the business-wide role. Owner only (checked by
/// the caller via `BusinessRoleManage`).
pub async fn set_business_role(
    pool: &PgPool,
    access: &Access,
    user_id: UserId,
    input: SetBusinessRole,
) -> AppResult<Employee> {
    if input.role.is_some_and(|role| !role.is_business_wide()) {
        return Err(AppError::field(
            "role",
            "must be business_owner, administrator, or null",
        ));
    }
    let mut tx = pool.begin().await?;
    load_user(&mut tx, access.business_id, user_id, true).await?;
    let current = active_business_role(&mut tx, user_id).await?;
    if current == input.role {
        return load(&mut tx, access.business_id, user_id).await;
    }
    if current == Some(Role::BusinessOwner) {
        ensure_other_owner(&mut tx, access.business_id, user_id).await?;
    }

    sqlx::query(
        "UPDATE role_assignments SET revoked_at = now()
         WHERE user_id = $1 AND store_id IS NULL AND revoked_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    if let Some(role) = input.role {
        auth::insert_role(
            &mut tx,
            access.business_id,
            user_id,
            None,
            role,
            access.user_id,
        )
        .await?;
    }
    AuditEntry::new(access.business_id, AuditAction::PermissionChanged)
        .actor(access.user_id)
        .record(RecordType::User, user_id)
        .before(&json!({ "business_role": current }))
        .after(&json!({ "business_role": input.role }))
        .record_in(&mut *tx)
        .await?;
    let updated = load(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}

/// Assigns the employee to a store, or changes their role there.
pub async fn assign_store(
    pool: &PgPool,
    access: &Access,
    user_id: UserId,
    store_id: StoreId,
    input: AssignStore,
) -> AppResult<Employee> {
    if input.role.is_business_wide() {
        return Err(AppError::field(
            "role",
            "must be store_manager, cashier, inventory_officer, or finance_officer",
        ));
    }
    let mut tx = pool.begin().await?;
    load_user(&mut tx, access.business_id, user_id, true).await?;
    ensure_can_modify(&mut tx, access, user_id).await?;
    ensure_stores_in_business(&mut tx, access.business_id, &[store_id]).await?;

    let previous: Option<Role> = sqlx::query_scalar(
        "UPDATE role_assignments SET revoked_at = now()
         WHERE user_id = $1 AND store_id = $2 AND revoked_at IS NULL
         RETURNING role",
    )
    .bind(user_id)
    .bind(store_id)
    .fetch_optional(&mut *tx)
    .await?;
    auth::insert_role(
        &mut tx,
        access.business_id,
        user_id,
        Some(store_id),
        input.role,
        access.user_id,
    )
    .await?;
    AuditEntry::new(access.business_id, AuditAction::PermissionChanged)
        .store(Some(store_id))
        .actor(access.user_id)
        .record(RecordType::User, user_id)
        .before(&json!({ "role": previous }))
        .after(&json!({ "role": input.role }))
        .record_in(&mut *tx)
        .await?;
    let updated = load(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}

pub async fn unassign_store(
    pool: &PgPool,
    access: &Access,
    user_id: UserId,
    store_id: StoreId,
) -> AppResult<Employee> {
    let mut tx = pool.begin().await?;
    load_user(&mut tx, access.business_id, user_id, true).await?;
    ensure_can_modify(&mut tx, access, user_id).await?;

    let removed: Option<Role> = sqlx::query_scalar(
        "UPDATE role_assignments SET revoked_at = now()
         WHERE business_id = $1 AND user_id = $2 AND store_id = $3 AND revoked_at IS NULL
         RETURNING role",
    )
    .bind(access.business_id)
    .bind(user_id)
    .bind(store_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(removed) = removed else {
        return Err(AppError::NotFound("store assignment"));
    };
    AuditEntry::new(access.business_id, AuditAction::PermissionChanged)
        .store(Some(store_id))
        .actor(access.user_id)
        .record(RecordType::User, user_id)
        .before(&json!({ "role": removed }))
        .after(&json!({ "role": null }))
        .record_in(&mut *tx)
        .await?;
    let updated = load(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}
