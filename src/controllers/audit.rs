use axum::extract::State;
use uuid::Uuid;

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{audit::AuditLog, roles::Permission},
    services::{
        access::authorize,
        audit::{self, AuditFilter},
    },
    state::AppState,
    utils::extract::{Json, Path, Query},
};

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<Uuid>,
    Query(filter): Query<AuditFilter>,
) -> AppResult<Json<Vec<AuditLog>>> {
    authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::AuditLogView,
    )
    .await?;
    Ok(Json(audit::list(&state.pool, business_id, filter).await?))
}
