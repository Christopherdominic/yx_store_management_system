//! End-to-end tests for FR-01 – FR-06. Each test gets a fresh database from
//! `#[sqlx::test]`; set DATABASE_URL to a Postgres server (see README).

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use xy_store::{config::Config, state::AppState};

const PASSWORD: &str = "Passw0rd!";

fn address(city: &str) -> Value {
    json!({ "line1": format!("1 Main Rd, {city}"), "city": city, "country": "NG" })
}

struct TestApp {
    router: Router,
}

impl TestApp {
    fn new(pool: PgPool) -> Self {
        Self {
            router: xy_store::app(AppState::new(pool, Config::for_tests())),
        }
    }

    async fn call(
        &self,
        method: Method,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"));
        if let Some(token) = token {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        let req = match body {
            Some(body) => req
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    async fn get(&self, uri: &str, token: &str) -> (StatusCode, Value) {
        self.call(Method::GET, uri, Some(token), None).await
    }

    async fn post(&self, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, uri, Some(token), Some(body)).await
    }

    async fn patch(&self, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, uri, Some(token), Some(body)).await
    }

    async fn put(&self, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, uri, Some(token), Some(body)).await
    }

    async fn login(&self, email: &str, password: &str) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "email": email, "password": password })),
        )
        .await
    }

    async fn token(&self, email: &str) -> String {
        let (status, body) = self.login(email, PASSWORD).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["token"].as_str().unwrap().to_string()
    }

    /// Registers a business with its owner; returns (owner token, business id).
    async fn register(&self, email: &str, name: &str) -> (String, String) {
        let (status, body) = self
            .call(
                Method::POST,
                "/auth/register",
                None,
                Some(json!({
                    "business": {
                        "legal_name": format!("{name} Ltd"),
                        "trading_name": name,
                        "address": address("Lagos"),
                    },
                    "owner": { "full_name": "Owner", "email": email, "password": PASSWORD },
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let business = body["business"]["id"].as_str().unwrap().to_string();
        (self.token(email).await, business)
    }

    async fn store(&self, token: &str, business: &str, code: &str, name: &str) -> String {
        let (status, body) = self
            .post(
                &format!("/businesses/{business}/stores"),
                token,
                json!({ "code": code, "name": name, "address": address(name) }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    async fn employee(&self, token: &str, business: &str, email: &str, body: Value) -> String {
        let mut payload = json!({ "full_name": email, "email": email, "password": PASSWORD });
        payload
            .as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        let (status, body) = self
            .post(&format!("/businesses/{business}/employees"), token, payload)
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["id"].as_str().unwrap().to_string()
    }
}

/// Amiable Retail with Lagos, Abuja, and Kaduna stores.
struct Amiable {
    owner: String,
    business: String,
    lagos: String,
    abuja: String,
    kaduna: String,
}

async fn amiable(app: &TestApp) -> Amiable {
    let (owner, business) = app.register("owner@amiable.ng", "Amiable Retail").await;
    let lagos = app.store(&owner, &business, "LAG", "Lagos").await;
    let abuja = app.store(&owner, &business, "ABJ", "Abuja").await;
    let kaduna = app.store(&owner, &business, "KAD", "Kaduna").await;
    Amiable {
        owner,
        business,
        lagos,
        abuja,
        kaduna,
    }
}

// FR-04
#[sqlx::test]
async fn login_logout_and_revoked_session(pool: PgPool) {
    let app = TestApp::new(pool);

    let (status, _) = app.call(Method::GET, "/auth/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app.get("/auth/me", "not-a-real-token").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (token, business) = app.register("ada@example.com", "Ada Stores").await;
    let (status, me) = app.get("/auth/me", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["employee"]["email"], "ada@example.com");
    assert_eq!(me["employee"]["business_role"], "business_owner");
    assert_eq!(me["business"]["id"], business.as_str());
    assert!(me["employee"]["last_login_at"].is_string());

    let (status, _) = app.login("ADA@example.com", "wrong-pass1").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app
        .call(Method::POST, "/auth/logout", Some(&token), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.get("/auth/me", &token).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "revoked session must not work"
    );
}

// FR-01
#[sqlx::test]
async fn registration_is_validated(pool: PgPool) {
    let app = TestApp::new(pool);
    let (status, body) = app
        .call(
            Method::POST,
            "/auth/register",
            None,
            Some(json!({
                "business": {
                    "legal_name": "",
                    "address": { "line1": "x", "city": "Lagos", "country": "Nigeria" },
                    "base_currency": "ABC",
                },
                "owner": { "full_name": "O", "email": "not-an-email", "password": "short" },
            })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let fields: Vec<&str> = body["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["field"].as_str().unwrap())
        .collect();
    assert_eq!(
        fields,
        [
            "base_currency",
            "legal_name",
            "address.country",
            "owner.email",
            "owner.password"
        ]
    );
}

// FR-01, FR-02
#[sqlx::test]
async fn owner_manages_business_and_three_stores(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;

    let (status, body) = app
        .patch(
            &format!("/businesses/{}", a.business),
            &a.owner,
            json!({ "registration_number": "RC123456" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["registration_number"], "RC123456");

    let (status, body) = app
        .patch(
            &format!("/businesses/{}/stores/{}", a.business, a.lagos),
            &a.owner,
            json!({ "name": "Lagos Island", "phone": "+2348000000000" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Lagos Island");
    assert_eq!(body["contact"]["phone"], "+2348000000000");

    let (status, body) = app
        .post(
            &format!("/businesses/{}/stores/{}/deactivate", a.business, a.kaduna),
            &a.owner,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "inactive");

    let (_, stores) = app
        .get(&format!("/businesses/{}/stores", a.business), &a.owner)
        .await;
    assert_eq!(
        stores.as_array().unwrap().len(),
        3,
        "deactivated store is kept"
    );
    let (_, found) = app
        .get(
            &format!("/businesses/{}/stores?q=abj", a.business),
            &a.owner,
        )
        .await;
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["id"], a.abuja.as_str());
    let (_, found) = app
        .get(
            &format!("/businesses/{}/stores?q={}", a.business, a.kaduna),
            &a.owner,
        )
        .await;
    assert_eq!(found.as_array().unwrap().len(), 1, "search by identifier");
    let (_, inactive) = app
        .get(
            &format!("/businesses/{}/stores?status=inactive", a.business),
            &a.owner,
        )
        .await;
    assert_eq!(inactive.as_array().unwrap().len(), 1);

    let (status, _) = app
        .post(
            &format!("/businesses/{}/stores", a.business),
            &a.owner,
            json!({ "code": "lag", "name": "Dup", "address": address("Ikeja") }),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "store codes are unique per business"
    );

    // Changes are attributed to a user and time.
    let (_, logs) = app
        .get(
            &format!(
                "/businesses/{}/audit-logs?action=updated&record_id={}",
                a.business, a.business
            ),
            &a.owner,
        )
        .await;
    assert_eq!(logs[0]["actor"]["type"], "user");
    assert_eq!(logs[0]["before"]["registration_number"], Value::Null);
    assert_eq!(logs[0]["after"]["registration_number"], "RC123456");
}

// FR-01, FR-06
#[sqlx::test]
async fn other_business_is_isolated(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let (rival, _) = app.register("owner@rival.ng", "Rival").await;

    for uri in [
        format!("/businesses/{}", a.business),
        format!("/businesses/{}/settings", a.business),
        format!("/businesses/{}/stores", a.business),
        format!("/businesses/{}/stores/{}", a.business, a.lagos),
        format!("/businesses/{}/employees", a.business),
    ] {
        let (status, _) = app.get(&uri, &rival).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    let (status, _) = app
        .patch(
            &format!("/businesses/{}", a.business),
            &rival,
            json!({ "trading_name": "Hijacked" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// FR-05, FR-06, FR-03
#[sqlx::test]
async fn roles_are_per_store(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    app.employee(
        &a.owner,
        &a.business,
        "ada@amiable.ng",
        json!({ "stores": [
            { "store_id": a.lagos, "role": "store_manager" },
            { "store_id": a.abuja, "role": "cashier" },
        ] }),
    )
    .await;
    let ada = app.token("ada@amiable.ng").await;
    let settings = |store: &str| format!("/businesses/{}/stores/{store}/settings", a.business);

    // Manager in Lagos.
    let (status, body) = app
        .patch(
            &settings(&a.lagos),
            &ada,
            json!({ "receipt_prefix": "LAGOS" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["receipt_prefix"], "LAGOS");
    // Cashier in Abuja.
    let (status, body) = app.get(&settings(&a.abuja), &ada).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["receipt_prefix"], "ABJ", "defaults to the store code");
    let (status, _) = app
        .patch(&settings(&a.abuja), &ada, json!({ "receipt_prefix": "X" }))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Not assigned to Kaduna at all.
    let (status, _) = app
        .get(
            &format!("/businesses/{}/stores/{}", a.business, a.kaduna),
            &ada,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Business-wide settings need an owner/administrator.
    let (status, _) = app
        .patch(
            &format!("/businesses/{}/settings", a.business),
            &ada,
            json!({ "base_currency": "USD" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Cannot create stores.
    let (status, _) = app
        .post(
            &format!("/businesses/{}/stores", a.business),
            &ada,
            json!({ "code": "X", "name": "X", "address": address("X") }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, stores) = app
        .get(&format!("/businesses/{}/stores", a.business), &ada)
        .await;
    let mut names: Vec<&str> = stores
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["Abuja", "Lagos"]);
}

// FR-06 acceptance criterion
#[sqlx::test]
async fn lagos_cashier_cannot_reach_abuja_and_attempt_is_logged(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let bayo_id = app
        .employee(
            &a.owner,
            &a.business,
            "bayo@amiable.ng",
            json!({ "stores": [{ "store_id": a.lagos, "role": "cashier" }] }),
        )
        .await;
    let bayo = app.token("bayo@amiable.ng").await;

    let (status, _) = app
        .get(
            &format!("/businesses/{}/stores/{}", a.business, a.lagos),
            &bayo,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = app
        .get(
            &format!("/businesses/{}/stores/{}", a.business, a.abuja),
            &bayo,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    let (status, _) = app
        .patch(
            &format!("/businesses/{}/stores/{}", a.business, a.abuja),
            &bayo,
            json!({ "name": "Mine" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (_, logs) = app
        .get(
            &format!(
                "/businesses/{}/audit-logs?action=access_denied&actor_user_id={bayo_id}",
                a.business
            ),
            &a.owner,
        )
        .await;
    let logs = logs.as_array().unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|l| l["store_id"] == a.abuja.as_str()));
    assert!(logs.iter().all(|l| l["outcome"] == "denied"));

    // Cashiers cannot read the audit trail.
    let (status, _) = app
        .get(&format!("/businesses/{}/audit-logs", a.business), &bayo)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

// FR-03
#[sqlx::test]
async fn settings_are_validated_and_audited(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let uri = format!("/businesses/{}/settings", a.business);

    let (status, body) = app
        .patch(
            &uri,
            &a.owner,
            json!({ "base_currency": "XYZ", "timezone": "Mars/Base", "tax_rate": "100.5" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let fields: Vec<&str> = body["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["field"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["base_currency", "timezone", "tax_rate"]);

    let (status, body) = app
        .patch(
            &uri,
            &a.owner,
            json!({ "base_currency": "usd", "tax_rate": "7.5", "receipt_footer": "Thank you" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["base_currency"], "USD");
    assert_eq!(body["tax_rate"], "7.5000");
    assert_eq!(body["receipt_footer"], "Thank you");

    let (_, business) = app
        .get(&format!("/businesses/{}", a.business), &a.owner)
        .await;
    assert_eq!(business["base_currency"], "USD");

    let (_, logs) = app
        .get(
            &format!(
                "/businesses/{}/audit-logs?action=settings_changed",
                a.business
            ),
            &a.owner,
        )
        .await;
    assert_eq!(logs[0]["before"]["tax_rate"], "0");
    assert_eq!(logs[0]["after"]["tax_rate"], "7.5000");

    let (status, _) = app
        .patch(
            &format!("/businesses/{}/stores/{}/settings", a.business, a.lagos),
            &a.owner,
            json!({ "receipt_prefix": "bad prefix!" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// NFR-16
#[sqlx::test]
async fn account_locks_after_five_failed_logins(pool: PgPool) {
    let app = TestApp::new(pool);
    app.register("lock@example.com", "Lock").await;
    for _ in 0..5 {
        let (status, _) = app.login("lock@example.com", "wrong-pass1").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _) = app.login("lock@example.com", PASSWORD).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

// FR-04
#[sqlx::test]
async fn password_reset_is_single_use_and_revokes_sessions(pool: PgPool) {
    let app = TestApp::new(pool);
    let (old_session, _) = app.register("reset@example.com", "Reset").await;

    let (status, body) = app
        .call(
            Method::POST,
            "/auth/password/forgot",
            None,
            Some(json!({ "email": "reset@example.com" })),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let reset_token = body["reset_token"].as_str().unwrap().to_string();

    let (status, unknown) = app
        .call(
            Method::POST,
            "/auth/password/forgot",
            None,
            Some(json!({ "email": "nobody@example.com" })),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        unknown["message"], body["message"],
        "must not reveal whether an email exists"
    );

    let reset = json!({ "token": reset_token, "new_password": "N3wPassword" });
    let (status, _) = app
        .call(
            Method::POST,
            "/auth/password/reset",
            None,
            Some(reset.clone()),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app
        .call(Method::POST, "/auth/password/reset", None, Some(reset))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "reset token is single use");

    let (status, _) = app.get("/auth/me", &old_session).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app.login("reset@example.com", PASSWORD).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app.login("reset@example.com", "N3wPassword").await;
    assert_eq!(status, StatusCode::OK);
}

// FR-04
#[sqlx::test]
async fn change_password_signs_out_other_sessions(pool: PgPool) {
    let app = TestApp::new(pool);
    let (first, _) = app.register("change@example.com", "Change").await;
    let second = app.token("change@example.com").await;

    let (status, _) = app
        .post(
            "/auth/password/change",
            &first,
            json!({ "current_password": "wrong", "new_password": "An0therPass" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = app
        .post(
            "/auth/password/change",
            &first,
            json!({ "current_password": PASSWORD, "new_password": "An0therPass" }),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    assert_eq!(app.get("/auth/me", &first).await.0, StatusCode::OK);
    assert_eq!(
        app.get("/auth/me", &second).await.0,
        StatusCode::UNAUTHORIZED
    );
}

// FR-05
#[sqlx::test]
async fn employee_lifecycle_and_role_guards(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let owner_id = app.get("/auth/me", &a.owner).await.1["employee"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let base = format!("/businesses/{}/employees", a.business);

    let admin_id = app
        .employee(
            &a.owner,
            &a.business,
            "admin@amiable.ng",
            json!({ "business_role": "administrator" }),
        )
        .await;
    let admin = app.token("admin@amiable.ng").await;
    let chidi_id = app
        .employee(
            &admin,
            &a.business,
            "chidi@amiable.ng",
            json!({ "stores": [{ "store_id": a.kaduna, "role": "inventory_officer" }] }),
        )
        .await;
    let chidi = app.token("chidi@amiable.ng").await;

    // Duplicate emails are rejected.
    let (status, _) = app
        .post(
            &base,
            &admin,
            json!({ "full_name": "Dup", "email": "CHIDI@amiable.ng", "password": PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Store roles cannot be business-wide and vice versa.
    let (status, _) = app
        .post(
            &base,
            &a.owner,
            json!({ "full_name": "X", "email": "x@amiable.ng", "password": PASSWORD,
                    "stores": [{ "store_id": a.lagos, "role": "administrator" }] }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Administrators cannot grant business roles or touch the owner.
    let (status, _) = app
        .post(
            &base,
            &admin,
            json!({ "full_name": "X", "email": "x@amiable.ng", "password": PASSWORD,
                    "business_role": "administrator" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .post(&format!("{base}/{owner_id}/deactivate"), &admin, json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A second store role, then a role change at the first store.
    let (status, body) = app
        .put(
            &format!("{base}/{chidi_id}/stores/{}", a.lagos),
            &admin,
            json!({ "role": "finance_officer" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stores"].as_array().unwrap().len(), 2);
    let (status, body) = app
        .put(
            &format!("{base}/{chidi_id}/stores/{}", a.kaduna),
            &admin,
            json!({ "role": "cashier" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let kaduna = body["stores"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["store_id"] == a.kaduna.as_str())
        .unwrap();
    assert_eq!(kaduna["role"], "cashier");

    // Deactivation revokes sessions and blocks login and business access.
    let (status, body) = app
        .post(&format!("{base}/{chidi_id}/deactivate"), &admin, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "deactivated");
    assert_eq!(
        app.get("/auth/me", &chidi).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login("chidi@amiable.ng", PASSWORD).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, _) = app
        .post(&format!("{base}/{chidi_id}/reactivate"), &admin, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let chidi = app.token("chidi@amiable.ng").await;
    let (status, _) = app
        .get(
            &format!("/businesses/{}/stores/{}", a.business, a.kaduna),
            &chidi,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Owner protections.
    let (status, _) = app
        .post(
            &format!("{base}/{owner_id}/deactivate"),
            &a.owner,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "cannot deactivate yourself");
    let (status, _) = app
        .put(
            &format!("{base}/{owner_id}/business-role"),
            &a.owner,
            json!({ "role": null }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "last owner cannot be removed");
    let (status, _) = app
        .put(
            &format!("{base}/{admin_id}/business-role"),
            &admin,
            json!({ "role": "business_owner" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = app
        .put(
            &format!("{base}/{admin_id}/business-role"),
            &a.owner,
            json!({ "role": null }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["business_role"], Value::Null);
    assert_eq!(
        app.get(&base, &admin).await.0,
        StatusCode::FORBIDDEN,
        "takes effect immediately"
    );
}

// FR-05: managers see only staff at their stores.
#[sqlx::test]
async fn manager_sees_only_their_store_staff(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    app.employee(
        &a.owner,
        &a.business,
        "mgr@amiable.ng",
        json!({ "stores": [{ "store_id": a.lagos, "role": "store_manager" }] }),
    )
    .await;
    app.employee(
        &a.owner,
        &a.business,
        "lag@amiable.ng",
        json!({ "stores": [{ "store_id": a.lagos, "role": "cashier" }] }),
    )
    .await;
    let abj_id = app
        .employee(
            &a.owner,
            &a.business,
            "abj@amiable.ng",
            json!({ "stores": [{ "store_id": a.abuja, "role": "cashier" }] }),
        )
        .await;
    let mgr = app.token("mgr@amiable.ng").await;

    let (status, list) = app
        .get(&format!("/businesses/{}/employees", a.business), &mgr)
        .await;
    assert_eq!(status, StatusCode::OK);
    let mut emails: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["email"].as_str().unwrap())
        .collect();
    emails.sort();
    assert_eq!(emails, ["lag@amiable.ng", "mgr@amiable.ng"]);
    let (status, _) = app
        .get(
            &format!("/businesses/{}/employees/{abj_id}", a.business),
            &mgr,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
