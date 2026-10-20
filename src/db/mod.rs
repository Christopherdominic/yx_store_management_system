//! PostgreSQL mappings for the domain models.
//!
//! The models in `crate::models` stay free of database concerns; this module
//! teaches SQLx how to bind and read them:
//! - typed IDs are stored as `UUID`;
//! - unit-like enums are stored as `TEXT`, using their serde (snake_case) name;
//! - records are read with explicit `FromRow` impls because some fields
//!   (address, contact, audit actor/record) span several columns or JSONB.

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{
    Decode, Encode, FromRow, Postgres, Row, Type,
    encode::IsNull,
    error::BoxDynError,
    postgres::{PgHasArrayType, PgRow, PgTypeInfo, PgValueRef},
    types::Json,
};
use uuid::Uuid;

use crate::models::{
    Address, AuditAction, AuditActor, AuditLog, AuditLogId, AuditOutcome, Business, BusinessId,
    BusinessStatus, ContactInfo, CurrencyCode, RecordRef, RecordType, Role, RoleAssignment,
    RoleAssignmentId, Store, StoreId, StoreStatus, User, UserId, UserStatus,
};

type ArgumentBuffer = <Postgres as sqlx::Database>::ArgumentBuffer;

macro_rules! uuid_column {
    ($($id:ty),+ $(,)?) => {$(
        impl Type<Postgres> for $id {
            fn type_info() -> PgTypeInfo {
                <Uuid as Type<Postgres>>::type_info()
            }

            fn compatible(ty: &PgTypeInfo) -> bool {
                <Uuid as Type<Postgres>>::compatible(ty)
            }
        }

        impl PgHasArrayType for $id {
            fn array_type_info() -> PgTypeInfo {
                <Uuid as PgHasArrayType>::array_type_info()
            }
        }

        impl Encode<'_, Postgres> for $id {
            fn encode_by_ref(&self, buf: &mut ArgumentBuffer) -> Result<IsNull, BoxDynError> {
                <Uuid as Encode<Postgres>>::encode_by_ref(self.as_uuid(), buf)
            }
        }

        impl<'r> Decode<'r, Postgres> for $id {
            fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
                Ok(Self::from_uuid(<Uuid as Decode<Postgres>>::decode(value)?))
            }
        }
    )+};
}

uuid_column!(BusinessId, StoreId, UserId, RoleAssignmentId, AuditLogId);

fn to_text<T: Serialize>(value: &T) -> Result<String, BoxDynError> {
    match serde_json::to_value(value)? {
        Value::String(text) => Ok(text),
        other => Err(format!("expected a string-like enum, got {other}").into()),
    }
}

fn from_text<T: DeserializeOwned>(text: &str) -> Result<T, BoxDynError> {
    Ok(serde_json::from_value(Value::String(text.to_owned()))?)
}

macro_rules! text_column {
    ($($ty:ty),+ $(,)?) => {$(
        impl Type<Postgres> for $ty {
            fn type_info() -> PgTypeInfo {
                <String as Type<Postgres>>::type_info()
            }

            fn compatible(ty: &PgTypeInfo) -> bool {
                <String as Type<Postgres>>::compatible(ty)
            }
        }

        impl Encode<'_, Postgres> for $ty {
            fn encode_by_ref(&self, buf: &mut ArgumentBuffer) -> Result<IsNull, BoxDynError> {
                <String as Encode<Postgres>>::encode(to_text(self)?, buf)
            }
        }

        impl<'r> Decode<'r, Postgres> for $ty {
            fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
                from_text(<&str as Decode<Postgres>>::decode(value)?)
            }
        }
    )+};
}

text_column!(
    AuditAction,
    AuditOutcome,
    BusinessStatus,
    CurrencyCode,
    RecordType,
    Role,
    StoreStatus,
    UserStatus,
);

impl<'r> FromRow<'r, PgRow> for Business {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            legal_name: row.try_get("legal_name")?,
            trading_name: row.try_get("trading_name")?,
            registration_number: row.try_get("registration_number")?,
            address: row.try_get::<Json<Address>, _>("address")?.0,
            contact: contact(row)?,
            logo_url: row.try_get("logo_url")?,
            base_currency: row.try_get("base_currency")?,
            timezone: row.try_get("timezone")?,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for Store {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            business_id: row.try_get("business_id")?,
            code: row.try_get("code")?,
            name: row.try_get("name")?,
            address: row.try_get::<Json<Address>, _>("address")?.0,
            contact: contact(row)?,
            status: row.try_get("status")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for User {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            business_id: row.try_get("business_id")?,
            full_name: row.try_get("full_name")?,
            email: row.try_get("email")?,
            phone: row.try_get("phone")?,
            status: row.try_get("status")?,
            last_login_at: row.try_get("last_login_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for RoleAssignment {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            business_id: row.try_get("business_id")?,
            user_id: row.try_get("user_id")?,
            store_id: row.try_get("store_id")?,
            role: row.try_get("role")?,
            assigned_by: row.try_get("assigned_by")?,
            assigned_at: row.try_get("assigned_at")?,
            revoked_at: row.try_get("revoked_at")?,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for AuditLog {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let actor = match row.try_get::<Option<UserId>, _>("actor_user_id")? {
            Some(user_id) => AuditActor::User(user_id),
            None => AuditActor::System(
                row.try_get::<Option<String>, _>("actor_system")?
                    .unwrap_or_default(),
            ),
        };
        let record_type: Option<RecordType> = row.try_get("record_type")?;
        let record_id: Option<Uuid> = row.try_get("record_id")?;
        Ok(Self {
            id: row.try_get("id")?,
            business_id: row.try_get("business_id")?,
            store_id: row.try_get("store_id")?,
            actor,
            action: row.try_get("action")?,
            record: record_type
                .zip(record_id)
                .map(|(record_type, record_id)| RecordRef::new(record_type, record_id)),
            outcome: row.try_get("outcome")?,
            before: row.try_get("before_values")?,
            after: row.try_get("after_values")?,
            details: row.try_get("details")?,
            ip_address: row.try_get("ip_address")?,
            user_agent: row.try_get("user_agent")?,
            request_id: row.try_get("request_id")?,
            occurred_at: row.try_get("occurred_at")?,
        })
    }
}

fn contact(row: &PgRow) -> Result<ContactInfo, sqlx::Error> {
    Ok(ContactInfo {
        phone: row.try_get("phone")?,
        email: row.try_get("email")?,
    })
}
