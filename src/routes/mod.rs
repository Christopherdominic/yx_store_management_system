use axum::{
    Router,
    routing::{get, patch, post, put},
};
use tower_http::trace::TraceLayer;

use crate::{
    controllers::{audit, auth, businesses, categories, employees, products, stores},
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
        .merge(catalog())
}

/// Product catalogue (FR-07 – FR-09).
fn catalog() -> Router<AppState> {
    const CATEGORY: &str = "/businesses/{business_id}/product-categories/{category_id}";
    const PRODUCT: &str = "/businesses/{business_id}/products/{product_id}";
    const VARIANT: &str = "/businesses/{business_id}/products/{product_id}/variants/{variant_id}";

    Router::new()
        // Categories (FR-08)
        .route(
            "/businesses/{business_id}/product-categories",
            get(categories::list).post(categories::create),
        )
        .route(CATEGORY, get(categories::get).patch(categories::update))
        .route(&format!("{CATEGORY}/archive"), post(categories::archive))
        .route(&format!("{CATEGORY}/restore"), post(categories::restore))
        // Products (FR-07) and lookup by SKU or barcode (FR-08)
        .route(
            "/businesses/{business_id}/products",
            get(products::list).post(products::create),
        )
        .route(
            "/businesses/{business_id}/products/lookup",
            get(products::lookup),
        )
        .route(PRODUCT, get(products::get).patch(products::update))
        .route(&format!("{PRODUCT}/archive"), post(products::archive))
        .route(&format!("{PRODUCT}/restore"), post(products::restore))
        // Prices and price history (FR-09)
        .route(&format!("{PRODUCT}/prices"), patch(products::change_prices))
        .route(
            &format!("{PRODUCT}/price-history"),
            get(products::price_history),
        )
        // Variants (FR-08)
        .route(
            &format!("{PRODUCT}/variants"),
            get(products::list_variants).post(products::create_variant),
        )
        .route(
            VARIANT,
            get(products::get_variant).patch(products::update_variant),
        )
        .route(
            &format!("{VARIANT}/archive"),
            post(products::archive_variant),
        )
        .route(
            &format!("{VARIANT}/restore"),
            post(products::restore_variant),
        )
        .route(
            &format!("{VARIANT}/prices"),
            patch(products::change_variant_prices),
        )
}
