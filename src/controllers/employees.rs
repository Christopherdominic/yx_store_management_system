use axum::{extract::State, http::StatusCode};

use crate::{
    errors::AppResult,
    middlewares::auth::AuthUser,
    models::{BusinessId, Employee, Permission, StoreId, UserId, UserStatus},
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
    Path(business_id): Path<BusinessId>,
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
    Path(business_id): Path<BusinessId>,
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
    Path((business_id, user_id)): Path<(BusinessId, UserId)>,
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
    Path((business_id, user_id)): Path<(BusinessId, UserId)>,
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
    Path((business_id, user_id)): Path<(BusinessId, UserId)>,
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
        employees::set_status(&state.pool, &access, user_id, UserStatus::Deactivated).await?,
    ))
}

pub async fn reactivate(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id)): Path<(BusinessId, UserId)>,
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
        employees::set_status(&state.pool, &access, user_id, UserStatus::Active).await?,
    ))
}

pub async fn set_business_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path((business_id, user_id)): Path<(BusinessId, UserId)>,
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
    Path((business_id, user_id, store_id)): Path<(BusinessId, UserId, StoreId)>,
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
    Path((business_id, user_id, store_id)): Path<(BusinessId, UserId, StoreId)>,
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
