use axum::{
    Router,
    routing::{get, post, put},
};
use tower_http::trace::TraceLayer;

use crate::{
    controllers::{audit, auth, businesses, employees, stores},
    state::AppState,
};

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api/v1", api())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn api() -> Router<AppState> {
    Router::new()
        // Accounts and access control (FR-04)
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/logout-all", post(auth::logout_all))
        .route("/auth/me", get(auth::me))
        .route("/auth/password/change", post(auth::change_password))
        .route("/auth/password/forgot", post(auth::forgot_password))
        .route("/auth/password/reset", post(auth::reset_password))
        .route("/roles", get(auth::roles))
        // Businesses and settings (FR-01, FR-03)
        .route(
            "/businesses/{business_id}",
            get(businesses::get).patch(businesses::update),
        )
        .route(
            "/businesses/{business_id}/settings",
            get(businesses::get_settings).patch(businesses::update_settings),
        )
        // Stores (FR-02, FR-03)
        .route(
            "/businesses/{business_id}/stores",
            get(stores::list).post(stores::create),
        )
        .route(
            "/businesses/{business_id}/stores/{store_id}",
            get(stores::get).patch(stores::update),
        )
        .route(
            "/businesses/{business_id}/stores/{store_id}/activate",
            post(stores::activate),
        )
        .route(
            "/businesses/{business_id}/stores/{store_id}/deactivate",
            post(stores::deactivate),
        )
        .route(
            "/businesses/{business_id}/stores/{store_id}/settings",
            get(stores::get_settings).patch(stores::update_settings),
        )
        // Employees and roles (FR-05)
        .route(
            "/businesses/{business_id}/employees",
            get(employees::list).post(employees::create),
        )
        .route(
            "/businesses/{business_id}/employees/{user_id}",
            get(employees::get).patch(employees::update),
        )
        .route(
            "/businesses/{business_id}/employees/{user_id}/deactivate",
            post(employees::deactivate),
        )
        .route(
            "/businesses/{business_id}/employees/{user_id}/reactivate",
            post(employees::reactivate),
        )
        .route(
            "/businesses/{business_id}/employees/{user_id}/business-role",
            put(employees::set_business_role),
        )
        .route(
            "/businesses/{business_id}/employees/{user_id}/stores/{store_id}",
            put(employees::assign_store).delete(employees::unassign_store),
        )
        // Audit trail (FR-01, FR-06)
        .route("/businesses/{business_id}/audit-logs", get(audit::list))
}
