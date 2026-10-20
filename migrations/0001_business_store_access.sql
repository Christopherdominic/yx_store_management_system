-- FR-01 – FR-06: businesses, stores, settings, users, roles, sessions, audit.
-- Table shapes follow the domain models in src/models. Enum columns hold the
-- models' snake_case serde names.

CREATE TABLE businesses (
    id                  UUID PRIMARY KEY,
    legal_name          TEXT NOT NULL,
    trading_name        TEXT,
    registration_number TEXT,
    address             JSONB NOT NULL,
    phone               TEXT,
    email               TEXT,
    logo_url            TEXT,
    base_currency       TEXT NOT NULL CHECK (base_currency ~ '^[A-Z]{3}$'),
    timezone            TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspended')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- FR-03. tax_rate is a percentage: 7.5 = 7.5%.
CREATE TABLE business_settings (
    business_id        UUID PRIMARY KEY REFERENCES businesses(id),
    tax_name           TEXT NOT NULL DEFAULT 'VAT',
    tax_rate           NUMERIC(7, 4) NOT NULL DEFAULT 0 CHECK (tax_rate BETWEEN 0 AND 100),
    prices_include_tax BOOLEAN NOT NULL DEFAULT FALSE,
    tax_id             TEXT,
    receipt_header     TEXT,
    receipt_footer     TEXT,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE stores (
    id          UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id),
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,
    address     JSONB NOT NULL,
    phone       TEXT,
    email       TEXT,
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX stores_business_code_key ON stores (business_id, lower(code));
CREATE INDEX stores_business_id_idx ON stores (business_id);

CREATE TABLE store_settings (
    store_id           UUID PRIMARY KEY REFERENCES stores(id),
    receipt_prefix     TEXT NOT NULL,
    transaction_prefix TEXT NOT NULL,
    receipt_footer     TEXT,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id            UUID PRIMARY KEY,
    business_id   UUID NOT NULL REFERENCES businesses(id),
    full_name     TEXT NOT NULL,
    email         TEXT NOT NULL,
    phone         TEXT,
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'deactivated')),
    last_login_at TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_lower_key ON users (lower(email));
CREATE INDEX users_business_id_idx ON users (business_id);

-- Kept apart from users so credentials can never leak through a User row.
CREATE TABLE user_credentials (
    user_id               UUID PRIMARY KEY REFERENCES users(id),
    password_hash         TEXT NOT NULL,
    failed_login_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until          TIMESTAMPTZ,
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- FR-05. Business-wide roles have no store; store roles must have one.
-- Revoked rows are kept for history.
CREATE TABLE role_assignments (
    id          UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id),
    user_id     UUID NOT NULL REFERENCES users(id),
    store_id    UUID REFERENCES stores(id),
    role        TEXT NOT NULL CHECK (role IN ('business_owner', 'administrator', 'store_manager',
                                              'cashier', 'inventory_officer', 'finance_officer')),
    assigned_by UUID NOT NULL REFERENCES users(id),
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at  TIMESTAMPTZ,
    CHECK ((store_id IS NULL) = (role IN ('business_owner', 'administrator')))
);
-- At most one active business-wide role, and one active role per store.
CREATE UNIQUE INDEX role_assignments_business_role_key
    ON role_assignments (user_id) WHERE store_id IS NULL AND revoked_at IS NULL;
CREATE UNIQUE INDEX role_assignments_store_role_key
    ON role_assignments (user_id, store_id) WHERE store_id IS NOT NULL AND revoked_at IS NULL;
CREATE INDEX role_assignments_business_id_idx ON role_assignments (business_id);

-- Opaque bearer tokens; only the SHA-256 of the token is stored.
CREATE TABLE sessions (
    id           UUID PRIMARY KEY,
    user_id      UUID NOT NULL REFERENCES users(id),
    token_hash   TEXT NOT NULL UNIQUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ
);
CREATE INDEX sessions_user_id_idx ON sessions (user_id);

CREATE TABLE password_reset_tokens (
    id         UUID PRIMARY KEY,
    user_id    UUID NOT NULL REFERENCES users(id),
    token_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ
);

-- FR-27. No foreign keys: entries must survive, and denied requests may
-- name records that do not exist.
CREATE TABLE audit_logs (
    id            UUID PRIMARY KEY,
    business_id   UUID NOT NULL,
    store_id      UUID,
    actor_user_id UUID,
    actor_system  TEXT,
    action        TEXT NOT NULL,
    record_type   TEXT,
    record_id     UUID,
    outcome       TEXT NOT NULL CHECK (outcome IN ('success', 'failure', 'denied')),
    before_values JSONB,
    after_values  JSONB,
    details       TEXT,
    ip_address    TEXT,
    user_agent    TEXT,
    request_id    TEXT,
    occurred_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((actor_user_id IS NULL) <> (actor_system IS NULL))
);
CREATE INDEX audit_logs_business_occurred_idx ON audit_logs (business_id, occurred_at DESC);
CREATE INDEX audit_logs_record_idx ON audit_logs (record_id);

CREATE FUNCTION audit_logs_immutable() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'audit_logs is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_logs_no_update_delete
    BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION audit_logs_immutable();
