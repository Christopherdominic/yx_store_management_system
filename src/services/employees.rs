//! Employee accounts, business roles, and store assignments (FR-05).
//!
//! Rules beyond the permission table:
//! - Only a business owner may grant/revoke the owner or administrator role,
//!   or modify an employee who holds one.
//! - Nobody can deactivate themselves, and a business always keeps at least
//!   one active owner.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::{
        roles::{BusinessRole, Permission, StoreRole},
        user::{Employee, StoreAssignment},
    },
    services::{access::Access, audit::AuditEntry, auth},
    utils::{
        crypto::hash_password_async,
        validation::{Validator, clean, clean_opt, like_pattern, normalize_email, patch_opt},
    },
};

#[derive(Debug, Deserialize)]
pub struct StoreRoleInput {
    pub store_id: Uuid,
    pub role: StoreRole,
}

#[derive(Debug, Deserialize)]
pub struct CreateEmployee {
    pub full_name: String,
    pub email: String,
    pub phone: Option<String>,
    /// Initial password set by the administrator and shared with the employee.
    pub password: String,
    pub business_role: Option<BusinessRole>,
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
    pub role: Option<BusinessRole>,
}

#[derive(Debug, Deserialize)]
pub struct AssignStore {
    pub role: StoreRole,
}

#[derive(Debug, Default, Deserialize)]
pub struct EmployeeFilter {
    /// Matches name or email (partial).
    pub q: Option<String>,
    pub store_id: Option<Uuid>,
    pub is_active: Option<bool>,
}

#[derive(Debug, FromRow)]
struct EmployeeRow {
    user_id: Uuid,
    business_id: Uuid,
    email: String,
    full_name: String,
    phone: Option<String>,
    business_role: Option<BusinessRole>,
    is_active: bool,
    created_at: DateTime<Utc>,
}

async fn load_row(
    conn: &mut PgConnection,
    business_id: Uuid,
    user_id: Uuid,
) -> AppResult<EmployeeRow> {
    sqlx::query_as::<_, EmployeeRow>(
        "SELECT u.id AS user_id, bm.business_id, u.email, u.full_name, u.phone,
                bm.business_role, bm.is_active, u.created_at
         FROM business_members bm
         JOIN users u ON u.id = bm.user_id
         WHERE bm.business_id = $1 AND bm.user_id = $2
         FOR UPDATE OF bm",
    )
    .bind(business_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?
    .ok_or(AppError::NotFound("employee"))
}

async fn with_assignments(
    conn: &mut PgConnection,
    business_id: Uuid,
    rows: Vec<EmployeeRow>,
) -> AppResult<Vec<Employee>> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.user_id).collect();
    let assignments: Vec<(Uuid, Uuid, String, String, StoreRole)> = sqlx::query_as(
        "SELECT sa.user_id, sa.store_id, s.name, s.code, sa.role
         FROM store_assignments sa
         JOIN stores s ON s.id = sa.store_id
         WHERE s.business_id = $1 AND sa.user_id = ANY($2)
         ORDER BY s.name",
    )
    .bind(business_id)
    .bind(&ids)
    .fetch_all(conn)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Employee {
            stores: assignments
                .iter()
                .filter(|a| a.0 == r.user_id)
                .map(|(_, store_id, name, code, role)| StoreAssignment {
                    store_id: *store_id,
                    store_name: name.clone(),
                    store_code: code.clone(),
                    role: *role,
                })
                .collect(),
            user_id: r.user_id,
            business_id: r.business_id,
            email: r.email,
            full_name: r.full_name,
            phone: r.phone,
            business_role: r.business_role,
            is_active: r.is_active,
            created_at: r.created_at,
        })
        .collect())
}

async fn employee(
    conn: &mut PgConnection,
    business_id: Uuid,
    user_id: Uuid,
) -> AppResult<Employee> {
    let row = load_row(conn, business_id, user_id).await?;
    let mut list = with_assignments(conn, business_id, vec![row]).await?;
    Ok(list.remove(0))
}

/// Holders of a business-wide role can only be changed by an owner.
fn ensure_can_modify(access: &Access, target: &EmployeeRow) -> AppResult<()> {
    if target.business_role.is_some() && !access.can(Permission::BusinessRoleManage, None) {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn ensure_stores_in_business(
    conn: &mut PgConnection,
    business_id: Uuid,
    store_ids: &[Uuid],
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

pub async fn create(pool: &PgPool, access: &Access, input: CreateEmployee) -> AppResult<Employee> {
    if input.business_role.is_some() && !access.can(Permission::BusinessRoleManage, None) {
        return Err(AppError::Forbidden);
    }
    let full_name = clean(input.full_name);
    let email = normalize_email(&input.email);
    let phone = clean_opt(input.phone);
    let store_ids: Vec<Uuid> = input.stores.iter().map(|s| s.store_id).collect();

    let mut v = Validator::new();
    v.required("full_name", &full_name, 200);
    v.required("email", &email, 254);
    v.email("email", Some(&email));
    v.optional("phone", phone.as_deref(), 30);
    v.password("password", &input.password);
    v.check(
        store_ids.iter().collect::<HashSet<_>>().len() == store_ids.len(),
        "stores",
        "lists the same store more than once",
    );
    v.finish()?;

    let password_hash = hash_password_async(input.password).await?;
    let business_id = access.business_id;
    let actor = access.user_id;

    let mut tx = pool.begin().await?;
    ensure_stores_in_business(&mut tx, business_id, &store_ids).await?;
    let user = auth::insert_user(
        &mut *tx,
        &full_name,
        &email,
        phone.as_deref(),
        &password_hash,
        Some(actor),
    )
    .await?;
    sqlx::query(
        "INSERT INTO business_members (business_id, user_id, business_role) VALUES ($1, $2, $3)",
    )
    .bind(business_id)
    .bind(user.id)
    .bind(input.business_role)
    .execute(&mut *tx)
    .await?;
    for assignment in &input.stores {
        sqlx::query(
            "INSERT INTO store_assignments (store_id, user_id, role, created_by) VALUES ($1, $2, $3, $4)",
        )
        .bind(assignment.store_id)
        .bind(user.id)
        .bind(assignment.role)
        .bind(actor)
        .execute(&mut *tx)
        .await?;
    }
    let created = employee(&mut tx, business_id, user.id).await?;
    AuditEntry::new("employee.created", "user")
        .business(business_id)
        .actor(actor)
        .entity(user.id)
        .after(&created)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(created)
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
    let rows = sqlx::query_as::<_, EmployeeRow>(
        "SELECT u.id AS user_id, bm.business_id, u.email, u.full_name, u.phone,
                bm.business_role, bm.is_active, u.created_at
         FROM business_members bm
         JOIN users u ON u.id = bm.user_id
         WHERE bm.business_id = $1
           AND ($2::uuid[] IS NULL OR EXISTS (
                SELECT 1 FROM store_assignments sa WHERE sa.user_id = u.id AND sa.store_id = ANY($2)))
           AND ($3::text IS NULL OR u.full_name ILIKE $3 OR u.email ILIKE $3)
           AND ($4::uuid IS NULL OR EXISTS (
                SELECT 1 FROM store_assignments sa WHERE sa.user_id = u.id AND sa.store_id = $4))
           AND ($5::boolean IS NULL OR bm.is_active = $5)
         ORDER BY u.full_name",
    )
    .bind(access.business_id)
        .bind(visible)
        .bind(term.as_deref().map(like_pattern))
        .bind(filter.store_id)
        .bind(filter.is_active)
        .fetch_all(&mut *conn)
        .await?;
    with_assignments(&mut conn, access.business_id, rows).await
}

pub async fn get(pool: &PgPool, access: &Access, user_id: Uuid) -> AppResult<Employee> {
    let mut conn = pool.acquire().await?;
    let row: EmployeeRow = sqlx::query_as(
        "SELECT u.id AS user_id, bm.business_id, u.email, u.full_name, u.phone,
                bm.business_role, bm.is_active, u.created_at
         FROM business_members bm
         JOIN users u ON u.id = bm.user_id
         WHERE bm.business_id = $1 AND bm.user_id = $2",
    )
    .bind(access.business_id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound("employee"))?;
    let employee = with_assignments(&mut conn, access.business_id, vec![row])
        .await?
        .remove(0);

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
    user_id: Uuid,
    input: UpdateEmployee,
) -> AppResult<Employee> {
    let mut tx = pool.begin().await?;
    let target = load_row(&mut tx, access.business_id, user_id).await?;
    ensure_can_modify(access, &target)?;
    let before = employee(&mut tx, access.business_id, user_id).await?;

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
    let after = employee(&mut tx, access.business_id, user_id).await?;
    AuditEntry::new("employee.updated", "user")
        .business(access.business_id)
        .actor(access.user_id)
        .entity(user_id)
        .before(&before)
        .after(&after)
        .record(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(after)
}

/// Locks the active owners of a business and errors if `user_id` is the
/// only one.
async fn ensure_other_owner(
    conn: &mut PgConnection,
    business_id: Uuid,
    user_id: Uuid,
) -> AppResult<()> {
    let owners: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM business_members
         WHERE business_id = $1 AND business_role = 'business_owner' AND is_active
         FOR UPDATE",
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

/// Deactivates the employee in this business and signs them out everywhere.
pub async fn set_active(
    pool: &PgPool,
    access: &Access,
    user_id: Uuid,
    active: bool,
) -> AppResult<Employee> {
    if !active && user_id == access.user_id {
        return Err(AppError::Conflict(
            "you cannot deactivate your own account".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let target = load_row(&mut tx, access.business_id, user_id).await?;
    ensure_can_modify(access, &target)?;
    if target.is_active == active {
        drop(tx);
        return get(pool, access, user_id).await;
    }
    if !active && target.business_role == Some(BusinessRole::BusinessOwner) {
        ensure_other_owner(&mut tx, access.business_id, user_id).await?;
    }

    sqlx::query(
        "UPDATE business_members SET is_active = $3, updated_at = now()
         WHERE business_id = $1 AND user_id = $2",
    )
    .bind(access.business_id)
    .bind(user_id)
    .bind(active)
    .execute(&mut *tx)
    .await?;
    let revoked = if active {
        0
    } else {
        auth::revoke_sessions(&mut *tx, user_id, None).await?
    };
    let action = if active {
        "employee.reactivated"
    } else {
        "employee.deactivated"
    };
    AuditEntry::new(action, "user")
        .business(access.business_id)
        .actor(access.user_id)
        .entity(user_id)
        .before(&json!({ "is_active": target.is_active }))
        .after(&json!({ "is_active": active }))
        .metadata(json!({ "sessions_revoked": revoked }))
        .record(&mut *tx)
        .await?;
    let updated = employee(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}

/// Grants or removes a business-wide role. Owner only (checked by caller via
/// `BusinessRoleManage`).
pub async fn set_business_role(
    pool: &PgPool,
    access: &Access,
    user_id: Uuid,
    input: SetBusinessRole,
) -> AppResult<Employee> {
    let mut tx = pool.begin().await?;
    let target = load_row(&mut tx, access.business_id, user_id).await?;
    if target.business_role == input.role {
        drop(tx);
        return get(pool, access, user_id).await;
    }
    if target.business_role == Some(BusinessRole::BusinessOwner) {
        ensure_other_owner(&mut tx, access.business_id, user_id).await?;
    }

    sqlx::query(
        "UPDATE business_members SET business_role = $3, updated_at = now()
         WHERE business_id = $1 AND user_id = $2",
    )
    .bind(access.business_id)
    .bind(user_id)
    .bind(input.role)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new("employee.business_role_changed", "user")
        .business(access.business_id)
        .actor(access.user_id)
        .entity(user_id)
        .before(&json!({ "business_role": target.business_role }))
        .after(&json!({ "business_role": input.role }))
        .record(&mut *tx)
        .await?;
    let updated = employee(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}

/// Assigns the employee to a store, or changes their role there.
pub async fn assign_store(
    pool: &PgPool,
    access: &Access,
    user_id: Uuid,
    store_id: Uuid,
    input: AssignStore,
) -> AppResult<Employee> {
    let mut tx = pool.begin().await?;
    let target = load_row(&mut tx, access.business_id, user_id).await?;
    ensure_can_modify(access, &target)?;
    ensure_stores_in_business(&mut tx, access.business_id, &[store_id]).await?;

    let previous: Option<StoreRole> = sqlx::query_scalar(
        "SELECT role FROM store_assignments WHERE store_id = $1 AND user_id = $2",
    )
    .bind(store_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO store_assignments (store_id, user_id, role, created_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (store_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(store_id)
    .bind(user_id)
    .bind(input.role)
    .bind(access.user_id)
    .execute(&mut *tx)
    .await?;
    AuditEntry::new("employee.store_assigned", "user")
        .business(access.business_id)
        .store(Some(store_id))
        .actor(access.user_id)
        .entity(user_id)
        .before(&json!({ "role": previous }))
        .after(&json!({ "role": input.role }))
        .record(&mut *tx)
        .await?;
    let updated = employee(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}

pub async fn unassign_store(
    pool: &PgPool,
    access: &Access,
    user_id: Uuid,
    store_id: Uuid,
) -> AppResult<Employee> {
    let mut tx = pool.begin().await?;
    let target = load_row(&mut tx, access.business_id, user_id).await?;
    ensure_can_modify(access, &target)?;

    let removed: Option<StoreRole> = sqlx::query_scalar(
        "DELETE FROM store_assignments sa
         USING stores s
         WHERE sa.store_id = s.id AND s.business_id = $1 AND sa.store_id = $2 AND sa.user_id = $3
         RETURNING sa.role",
    )
    .bind(access.business_id)
    .bind(store_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(removed) = removed else {
        return Err(AppError::NotFound("store assignment"));
    };
    AuditEntry::new("employee.store_unassigned", "user")
        .business(access.business_id)
        .store(Some(store_id))
        .actor(access.user_id)
        .entity(user_id)
        .before(&json!({ "role": removed }))
        .record(&mut *tx)
        .await?;
    let updated = employee(&mut tx, access.business_id, user_id).await?;
    tx.commit().await?;
    Ok(updated)
}
