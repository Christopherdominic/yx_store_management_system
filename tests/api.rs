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

    /// Registers an owner and returns their token.
    async fn owner(&self, email: &str) -> String {
        let (status, body) = self
            .call(
                Method::POST,
                "/auth/register",
                None,
                Some(json!({ "full_name": "Owner", "email": email, "password": PASSWORD })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        self.token(email).await
    }

    async fn business(&self, token: &str, name: &str) -> String {
        let (status, body) = self
            .post(
                "/businesses",
                token,
                json!({ "legal_name": format!("{name} Ltd"), "trading_name": name, "address": "1 Broad St, Lagos" }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    async fn store(&self, token: &str, business: &str, code: &str, name: &str) -> String {
        let (status, body) = self
            .post(
                &format!("/businesses/{business}/stores"),
                token,
                json!({ "code": code, "name": name, "address": format!("{name} address") }),
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
        body["user_id"].as_str().unwrap().to_string()
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
    let owner = app.owner("owner@amiable.ng").await;
    let business = app.business(&owner, "Amiable Retail").await;
    let lagos = app.store(&owner, &business, "LAG-01", "Lagos").await;
    let abuja = app.store(&owner, &business, "ABJ-01", "Abuja").await;
    let kaduna = app.store(&owner, &business, "KAD-01", "Kaduna").await;
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

    let token = app.owner("ada@example.com").await;
    let (status, me) = app.get("/auth/me", &token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["email"], "ada@example.com");
    assert!(
        me["user"].get("password_hash").is_none(),
        "hash must never be returned"
    );

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
            json!({ "code": "lag-01", "name": "Dup", "address": "x" }),
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
                "/businesses/{}/audit-logs?action=business.updated",
                a.business
            ),
            &a.owner,
        )
        .await;
    assert_eq!(logs[0]["before_values"]["registration_number"], Value::Null);
    assert_eq!(logs[0]["after_values"]["registration_number"], "RC123456");
}

// FR-01, FR-06
#[sqlx::test]
async fn other_business_is_isolated(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let rival = app.owner("owner@rival.ng").await;
    app.business(&rival, "Rival").await;

    for uri in [
        format!("/businesses/{}", a.business),
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

    let (_, mine) = app.get("/businesses", &rival).await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
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
            json!({ "receipt_prefix": "LAG" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["receipt_prefix"], "LAG");
    // Cashier in Abuja.
    let (status, _) = app.get(&settings(&a.abuja), &ada).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .patch(
            &settings(&a.abuja),
            &ada,
            json!({ "receipt_prefix": "ABJ" }),
        )
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
    // Cannot create staff or stores.
    let (status, _) = app
        .post(
            &format!("/businesses/{}/stores", a.business),
            &ada,
            json!({ "code": "X", "name": "X", "address": "X" }),
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
                "/businesses/{}/audit-logs?outcome=denied&actor_user_id={bayo_id}",
                a.business
            ),
            &a.owner,
        )
        .await;
    let logs = logs.as_array().unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|l| l["store_id"] == a.abuja.as_str()));

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
            json!({ "base_currency": "XYZ", "timezone": "Mars/Base", "tax_rate_bps": 10001 }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let fields: Vec<&str> = body["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["field"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["base_currency", "timezone", "tax_rate_bps"]);

    let (status, body) = app
        .patch(
            &uri,
            &a.owner,
            json!({ "base_currency": "ngn", "tax_rate_bps": 750, "receipt_footer": "Thank you" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["base_currency"], "NGN");
    assert_eq!(body["tax_rate_bps"], 750);

    let (_, logs) = app
        .get(
            &format!(
                "/businesses/{}/audit-logs?action=business.settings_updated",
                a.business
            ),
            &a.owner,
        )
        .await;
    assert_eq!(logs[0]["before_values"]["tax_rate_bps"], 0);
    assert_eq!(logs[0]["after_values"]["tax_rate_bps"], 750);

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
    app.owner("lock@example.com").await;
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
    let old_session = app.owner("reset@example.com").await;

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
    let first = app.owner("change@example.com").await;
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
    let owner_id = app.get("/auth/me", &a.owner).await.1["user"]["id"]
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

    // Public sign-up cannot create staff, and duplicate emails are rejected.
    let (status, _) = app
        .post(
            &base,
            &admin,
            json!({ "full_name": "Dup", "email": "CHIDI@amiable.ng", "password": PASSWORD }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Administrators cannot grant business roles or touch the owner.
    let (status, _) = app
        .post(&base, &admin, json!({ "full_name": "X", "email": "x@amiable.ng", "password": PASSWORD, "business_role": "administrator" }))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .post(&format!("{base}/{owner_id}/deactivate"), &admin, json!({}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Reassign Chidi to a different role at another store.
    let (status, body) = app
        .put(
            &format!("{base}/{chidi_id}/stores/{}", a.lagos),
            &admin,
            json!({ "role": "finance_officer" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stores"].as_array().unwrap().len(), 2);

    // Deactivation revokes sessions and blocks business access.
    let (status, body) = app
        .post(&format!("{base}/{chidi_id}/deactivate"), &admin, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["is_active"], false);
    assert_eq!(
        app.get("/auth/me", &chidi).await.0,
        StatusCode::UNAUTHORIZED
    );
    let chidi = app.token("chidi@amiable.ng").await;
    let (status, _) = app
        .get(
            &format!("/businesses/{}/stores/{}", a.business, a.kaduna),
            &chidi,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .post(&format!("{base}/{chidi_id}/reactivate"), &admin, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
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
