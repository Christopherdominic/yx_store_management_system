//! Business- and store-level authorization (FR-06).
//!
//! Access is always derived from the database (the user's business and
//! active role assignments) for the business/store named in the URL. A store
//! ID the client supplies is never trusted on its own: it only grants
//! anything if the user's own assignments grant it.

use std::collections::HashMap;

use sqlx::PgPool;

use crate::{
    errors::{AppError, AppResult},
    models::{AuditAction, AuditOutcome, BusinessId, Permission, Role, StoreId, UserId},
    services::audit::AuditEntry,
    state::AppState,
};

#[derive(Debug, Clone)]
pub struct Access {
    pub user_id: UserId,
    pub business_id: BusinessId,
    pub business_role: Option<Role>,
    pub store_roles: HashMap<StoreId, Role>,
}

impl Access {
    /// `None` unless the user is an active member of the business.
    pub async fn load(
        pool: &PgPool,
        user_id: UserId,
        business_id: BusinessId,
    ) -> AppResult<Option<Self>> {
        let member: Option<(UserId,)> = sqlx::query_as(
            "SELECT id FROM users WHERE id = $1 AND business_id = $2 AND status = 'active'",
        )
        .bind(user_id)
        .bind(business_id)
        .fetch_optional(pool)
        .await?;
        if member.is_none() {
            return Ok(None);
        }

        let assignments: Vec<(Option<StoreId>, Role)> = sqlx::query_as(
            "SELECT store_id, role FROM role_assignments
             WHERE user_id = $1 AND business_id = $2 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(business_id)
        .fetch_all(pool)
        .await?;

        let mut access = Self {
            user_id,
            business_id,
            business_role: None,
            store_roles: HashMap::new(),
        };
        for (store_id, role) in assignments {
            match store_id {
                Some(store_id) => {
                    access.store_roles.insert(store_id, role);
                }
                None => access.business_role = Some(role),
            }
        }
        Ok(Some(access))
    }

    /// With `store_id`, whether the permission applies to that store. Without
    /// it, whether the user holds the permission anywhere in the business.
    pub fn can(&self, permission: Permission, store_id: Option<StoreId>) -> bool {
        if self.business_role.is_some_and(|role| role.has(permission)) {
            return true;
        }
        if permission == Permission::BusinessView {
            return true;
        }
        if !permission.is_store_scoped() {
            return false;
        }
        match store_id {
            Some(id) => self
                .store_roles
                .get(&id)
                .is_some_and(|role| role.has(permission)),
            None => self.store_roles.values().any(|role| role.has(permission)),
        }
    }

    /// Stores where the permission applies; `None` means every store.
    pub fn stores_with(&self, permission: Permission) -> Option<Vec<StoreId>> {
        if self.business_role.is_some_and(|role| role.has(permission)) {
            return None;
        }
        Some(
            self.store_roles
                .iter()
                .filter(|(_, role)| role.has(permission))
                .map(|(id, _)| *id)
                .collect(),
        )
    }
}

/// Loads the user's access to `business_id` and checks one permission,
/// logging and rejecting the request when it is missing.
pub async fn authorize(
    state: &AppState,
    user_id: UserId,
    business_id: BusinessId,
    store_id: Option<StoreId>,
    permission: Permission,
) -> AppResult<Access> {
    match Access::load(&state.pool, user_id, business_id).await? {
        Some(access) if access.can(permission, store_id) => Ok(access),
        _ => {
            tracing::warn!(%user_id, %business_id, ?store_id, ?permission, "access denied");
            AuditEntry::new(business_id, AuditAction::AccessDenied)
                .store(store_id)
                .actor(user_id)
                .outcome(AuditOutcome::Denied)
                .details(format!(
                    "missing permission {}",
                    permission_name(permission)
                ))
                .record_best_effort(&state.pool)
                .await;
            Err(AppError::Forbidden)
        }
    }
}

fn permission_name(permission: Permission) -> String {
    serde_json::to_value(permission)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
