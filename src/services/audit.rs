use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::audit::{AuditLog, AuditOutcome},
};

/// One audit-trail record. Built with the chained setters, then `record`ed —
/// inside the same transaction as the change it describes where possible.
pub struct AuditEntry {
    action: &'static str,
    entity_type: &'static str,
    business_id: Option<Uuid>,
    store_id: Option<Uuid>,
    actor_user_id: Option<Uuid>,
    entity_id: Option<Uuid>,
    outcome: AuditOutcome,
    before: Option<Value>,
    after: Option<Value>,
    metadata: Option<Value>,
}

impl AuditEntry {
    pub fn new(action: &'static str, entity_type: &'static str) -> Self {
        Self {
            action,
            entity_type,
            business_id: None,
            store_id: None,
            actor_user_id: None,
            entity_id: None,
            outcome: AuditOutcome::Success,
            before: None,
            after: None,
            metadata: None,
        }
    }

    pub fn business(mut self, id: Uuid) -> Self {
        self.business_id = Some(id);
        self
    }

    pub fn store(mut self, id: Option<Uuid>) -> Self {
        self.store_id = id;
        self
    }

    pub fn actor(mut self, id: Uuid) -> Self {
        self.actor_user_id = Some(id);
        self
    }

    pub fn entity(mut self, id: Uuid) -> Self {
        self.entity_id = Some(id);
        self
    }

    pub fn outcome(mut self, outcome: AuditOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Callers must not pass secrets; `User` already skips its password hash.
    pub fn before<T: Serialize>(mut self, value: &T) -> Self {
        self.before = serde_json::to_value(value).ok();
        self
    }

    pub fn after<T: Serialize>(mut self, value: &T) -> Self {
        self.after = serde_json::to_value(value).ok();
        self
    }

    pub fn metadata(mut self, value: Value) -> Self {
        self.metadata = Some(value);
        self
    }

    pub async fn record(self, executor: impl PgExecutor<'_>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO audit_logs (id, business_id, store_id, actor_user_id, action, entity_type,
                                     entity_id, outcome, before_values, after_values, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(Uuid::new_v4())
        .bind(self.business_id)
        .bind(self.store_id)
        .bind(self.actor_user_id)
        .bind(self.action)
        .bind(self.entity_type)
        .bind(self.entity_id)
        .bind(self.outcome)
        .bind(self.before)
        .bind(self.after)
        .bind(self.metadata)
        .execute(executor)
        .await?;
        Ok(())
    }

    /// For entries that must not fail the request (denials, failed logins).
    pub async fn record_best_effort(self, pool: &PgPool) {
        let action = self.action;
        if let Err(err) = self.record(pool).await {
            tracing::error!(error = ?err, action, "failed to write audit log");
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct AuditFilter {
    pub actor_user_id: Option<Uuid>,
    pub action: Option<String>,
    pub store_id: Option<Uuid>,
    pub entity_id: Option<Uuid>,
    pub outcome: Option<AuditOutcome>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(
    pool: &PgPool,
    business_id: Uuid,
    filter: AuditFilter,
) -> AppResult<Vec<AuditLog>> {
    let limit = filter.limit.unwrap_or(50).clamp(1, 200);
    let offset = filter.offset.unwrap_or(0).max(0);
    let logs = sqlx::query_as::<_, AuditLog>(
        "SELECT * FROM audit_logs
         WHERE business_id = $1
           AND ($2::uuid IS NULL OR actor_user_id = $2)
           AND ($3::text IS NULL OR action = $3)
           AND ($4::uuid IS NULL OR store_id = $4)
           AND ($5::uuid IS NULL OR entity_id = $5)
           AND ($6::text IS NULL OR outcome = $6)
           AND ($7::timestamptz IS NULL OR created_at >= $7)
           AND ($8::timestamptz IS NULL OR created_at < $8)
         ORDER BY created_at DESC, id
         LIMIT $9 OFFSET $10",
    )
    .bind(business_id)
    .bind(filter.actor_user_id)
    .bind(filter.action)
    .bind(filter.store_id)
    .bind(filter.entity_id)
    .bind(filter.outcome)
    .bind(filter.from)
    .bind(filter.to)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(logs)
}
