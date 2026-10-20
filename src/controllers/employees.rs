use axum::{extract::State, http::StatusCode};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{roles::Permission, user::Employee},
    services::{
        access::authorize,
        employees::{
            self, AssignStore, CreateEmployee, EmployeeFilter, SetBusinessRole, UpdateEmployee,
        },
    },
    state::AppState,
    utils::extract::{Json, Path, Query},
};

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<Uuid>,
    Json(input): Json<CreateEmployee>,
) -> AppResult<(StatusCode, Json<Employee>)> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeManage,
    )
    .await?;
    let employee = employees::create(&state.pool, &access, input).await?;
    Ok((StatusCode::CREATED, Json(employee)))
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(business_id): Path<Uuid>,
    Query(filter): Query<EmployeeFilter>,
) -> AppResult<Json<Vec<Employee>>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeView,
    )
    .await?;
    Ok(Json(employees::list(&state.pool, &access, filter).await?))
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Employee>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeView,
    )
    .await?;
    Ok(Json(employees::get(&state.pool, &access, user_id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateEmployee>,
) -> AppResult<Json<Employee>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeManage,
    )
    .await?;
    Ok(Json(
        employees::update(&state.pool, &access, user_id, input).await?,
    ))
}

pub async fn deactivate(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Employee>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeManage,
    )
    .await?;
    Ok(Json(
        employees::set_active(&state.pool, &access, user_id, false).await?,
    ))
}

pub async fn reactivate(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<Employee>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeManage,
    )
    .await?;
    Ok(Json(
        employees::set_active(&state.pool, &access, user_id, true).await?,
    ))
}

pub async fn set_business_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<SetBusinessRole>,
) -> AppResult<Json<Employee>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::BusinessRoleManage,
    )
    .await?;
    Ok(Json(
        employees::set_business_role(&state.pool, &access, user_id, input).await?,
    ))
}

pub async fn assign_store(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id, store_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(input): Json<AssignStore>,
) -> AppResult<Json<Employee>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeManage,
    )
    .await?;
    Ok(Json(
        employees::assign_store(&state.pool, &access, user_id, store_id, input).await?,
    ))
}

pub async fn unassign_store(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id, store_id)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<Json<Employee>> {
    let access = authorize(
        &state,
        user.user_id,
        business_id,
        None,
        Permission::EmployeeManage,
    )
    .await?;
    Ok(Json(
        employees::unassign_store(&state.pool, &access, user_id, store_id).await?,
    ))
}
