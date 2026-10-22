//! Helpers shared by the API test files. Each test gets a fresh database
//! from `#[sqlx::test]`; set DATABASE_URL to a Postgres server (see README).

#![allow(dead_code)]

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

pub const PASSWORD: &str = "Passw0rd!";

pub fn address(city: &str) -> Value {
    json!({ "line1": format!("1 Main Rd, {city}"), "city": city, "country": "NG" })
}

pub struct TestApp {
    router: Router,
}

impl TestApp {
    pub fn new(pool: PgPool) -> Self {
        Self {
            router: xy_store::app(AppState::new(pool, Config::for_tests())),
        }
    }

    pub async fn call(
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

    pub async fn get(&self, uri: &str, token: &str) -> (StatusCode, Value) {
        self.call(Method::GET, uri, Some(token), None).await
    }

    pub async fn post(&self, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, uri, Some(token), Some(body)).await
    }

    pub async fn patch(&self, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, uri, Some(token), Some(body)).await
    }

    pub async fn token(&self, email: &str) -> String {
        let (status, body) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "email": email, "password": PASSWORD })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["token"].as_str().unwrap().to_string()
    }

    /// Registers a business with its owner; returns (owner token, business id).
    pub async fn register(&self, email: &str, name: &str) -> (String, String) {
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

    pub async fn store(&self, token: &str, business: &str, code: &str, name: &str) -> String {
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

    /// Creates an employee with `body` merged into the defaults; returns their ID.
    pub async fn employee(&self, token: &str, business: &str, email: &str, body: Value) -> String {
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
pub struct Amiable {
    pub owner: String,
    pub business: String,
    pub lagos: String,
    pub abuja: String,
    pub kaduna: String,
}

pub async fn amiable(app: &TestApp) -> Amiable {
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

/// Field names from a 422 validation error body.
pub fn error_fields(body: &Value) -> Vec<String> {
    body["error"]["details"]
        .as_array()
        .map(|details| {
            details
                .iter()
                .map(|d| d["field"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}
