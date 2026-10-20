//! Business- and store-level authorization (FR-06).
//!
//! Access is always derived from the database (membership and store
//! assignments) for the business/store named in the URL. A store ID the
//! client supplies is never trusted on its own: it only grants anything if
//! the user's own records grant it.

use std::collections::HashMap;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::{AppError, AppResult},
    models::{
        audit::AuditOutcome,
        roles::{BusinessRole, Permission, StoreRole},
    },
    services::audit::AuditEntry,
    state::AppState,
};

#[derive(Debug, Clone)]
pub struct Access {
    pub user_id: Uuid,
    pub business_id: Uuid,
    pub business_role: Option<BusinessRole>,
    pub store_roles: HashMap<Uuid, StoreRole>,
}

impl Access {
    /// `None` when the user is not an active member of the business.
    pub async fn load(pool: &PgPool, user_id: Uuid, business_id: Uuid) -> AppResult<Option<Self>> {
        let membership: Option<(Option<BusinessRole>,)> = sqlx::query_as(
            "SELECT business_role FROM business_members
             WHERE business_id = $1 AND user_id = $2 AND is_active",
        )
        .bind(business_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
        let Some((business_role,)) = membership else {
            return Ok(None);
        };

        let store_roles: Vec<(Uuid, StoreRole)> = sqlx::query_as(
            "SELECT sa.store_id, sa.role
             FROM store_assignments sa
             JOIN stores s ON s.id = sa.store_id
             WHERE sa.user_id = $1 AND s.business_id = $2",
        )
        .bind(user_id)
        .bind(business_id)
        .fetch_all(pool)
        .await?;

        Ok(Some(Self {
            user_id,
            business_id,
            business_role,
            store_roles: store_roles.into_iter().collect(),
        }))
    }

    /// With `store_id`, whether the permission applies to that store. Without
    /// it, whether the user holds the permission anywhere in the business.
    pub fn can(&self, permission: Permission, store_id: Option<Uuid>) -> bool {
        if self
            .business_role
            .is_some_and(|role| role.permissions().contains(&permission))
        {
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
                .is_some_and(|role| role.permissions().contains(&permission)),
            None => self
                .store_roles
                .values()
                .any(|role| role.permissions().contains(&permission)),
        }
    }

    /// Stores where the permission applies; `None` means every store.
    pub fn stores_with(&self, permission: Permission) -> Option<Vec<Uuid>> {
        if self
            .business_role
            .is_some_and(|role| role.permissions().contains(&permission))
        {
            return None;
        }
        Some(
            self.store_roles
                .iter()
                .filter(|(_, role)| role.permissions().contains(&permission))
                .map(|(id, _)| *id)
                .collect(),
        )
    }
}

/// Loads the user's access to `business_id` and checks one permission,
/// logging and rejecting the request when it is missing.
pub async fn authorize(
    state: &AppState,
    user_id: Uuid,
    business_id: Uuid,
    store_id: Option<Uuid>,
    permission: Permission,
) -> AppResult<Access> {
    match Access::load(&state.pool, user_id, business_id).await? {
        Some(access) if access.can(permission, store_id) => Ok(access),
        _ => Err(deny(state, user_id, business_id, store_id, permission).await),
    }
}

/// Records a rejected cross-store/cross-business attempt and returns 403.
pub async fn deny(
    state: &AppState,
    user_id: Uuid,
    business_id: Uuid,
    store_id: Option<Uuid>,
    permission: Permission,
) -> AppError {
    tracing::warn!(%user_id, %business_id, ?store_id, ?permission, "access denied");
    AuditEntry::new("access.denied", "permission")
        .business(business_id)
        .store(store_id)
        .actor(user_id)
        .outcome(AuditOutcome::Denied)
        .metadata(json!({ "permission": permission }))
        .record_best_effort(&state.pool)
        .await;
    AppError::Forbidden
}
