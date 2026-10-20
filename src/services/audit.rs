use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::{
        AuditAction, AuditLog, AuditLogId, AuditOutcome, BusinessId, RecordType, StoreId, UserId,
    },
};

/// One audit-trail record ([`AuditLog`]). Built with the chained setters,
/// then `record`ed — inside the same transaction as the change it describes
/// where possible.
pub struct AuditEntry {
    business_id: BusinessId,
    action: AuditAction,
    store_id: Option<StoreId>,
    actor_user_id: Option<UserId>,
    record: Option<(RecordType, Uuid)>,
    outcome: AuditOutcome,
    before: Option<Value>,
    after: Option<Value>,
    details: Option<String>,
}

impl AuditEntry {
    pub fn new(business_id: BusinessId, action: AuditAction) -> Self {
        Self {
            business_id,
            action,
            store_id: None,
            actor_user_id: None,
            record: None,
            outcome: AuditOutcome::Success,
            before: None,
            after: None,
            details: None,
        }
    }

    pub fn store(mut self, id: Option<StoreId>) -> Self {
        self.store_id = id;
        self
    }

    pub fn actor(mut self, id: UserId) -> Self {
        self.actor_user_id = Some(id);
        self
    }

    pub fn record(mut self, record_type: RecordType, id: impl Into<Uuid>) -> Self {
        self.record = Some((record_type, id.into()));
        self
    }

    pub fn outcome(mut self, outcome: AuditOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Callers must not pass secrets; no model carries credentials.
    pub fn before<T: Serialize>(mut self, value: &T) -> Self {
        self.before = serde_json::to_value(value).ok();
        self
    }

    pub fn after<T: Serialize>(mut self, value: &T) -> Self {
        self.after = serde_json::to_value(value).ok();
        self
    }

    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub async fn record_in(self, executor: impl PgExecutor<'_>) -> Result<(), sqlx::Error> {
        // Entries without a user actor are attributed to the API itself.
        let actor_system = self.actor_user_id.is_none().then_some("api");
        sqlx::query(
            "INSERT INTO audit_logs (id, business_id, store_id, actor_user_id, actor_system, action,
                                     record_type, record_id, outcome, before_values, after_values,
                                     details)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(AuditLogId::generate())
        .bind(self.business_id)
        .bind(self.store_id)
        .bind(self.actor_user_id)
        .bind(actor_system)
        .bind(self.action)
        .bind(self.record.map(|r| r.0))
        .bind(self.record.map(|r| r.1))
        .bind(self.outcome)
        .bind(self.before)
        .bind(self.after)
        .bind(self.details)
        .execute(executor)
        .await?;
        Ok(())
    }

    /// For entries that must not fail the request (denials, failed logins).
    pub async fn record_best_effort(self, pool: &PgPool) {
        let action = self.action;
        if let Err(err) = self.record_in(pool).await {
            tracing::error!(error = ?err, ?action, "failed to write audit log");
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct AuditFilter {
    pub actor_user_id: Option<UserId>,
    pub action: Option<AuditAction>,
    pub store_id: Option<StoreId>,
    pub record_id: Option<Uuid>,
    pub outcome: Option<AuditOutcome>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(
    pool: &PgPool,
    business_id: BusinessId,
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
           AND ($5::uuid IS NULL OR record_id = $5)
           AND ($6::text IS NULL OR outcome = $6)
           AND ($7::timestamptz IS NULL OR occurred_at >= $7)
           AND ($8::timestamptz IS NULL OR occurred_at < $8)
         ORDER BY occurred_at DESC, id
         LIMIT $9 OFFSET $10",
    )
    .bind(business_id)
    .bind(filter.actor_user_id)
    .bind(filter.action)
    .bind(filter.store_id)
    .bind(filter.record_id)
    .bind(filter.outcome)
    .bind(filter.from)
    .bind(filter.to)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(logs)
}
