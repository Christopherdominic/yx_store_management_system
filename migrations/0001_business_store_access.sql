-- FR-01 – FR-06: businesses, stores, settings, users, roles, sessions, audit.

CREATE TABLE users (
    id                    UUID PRIMARY KEY,
    email                 TEXT NOT NULL,
    full_name             TEXT NOT NULL,
    phone                 TEXT,
    password_hash         TEXT NOT NULL,
    is_active             BOOLEAN NOT NULL DEFAULT TRUE,
    failed_login_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until          TIMESTAMPTZ,
    created_by            UUID REFERENCES users(id),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_lower_key ON users (lower(email));

CREATE TABLE businesses (
    id                  UUID PRIMARY KEY,
    legal_name          TEXT NOT NULL,
    trading_name        TEXT NOT NULL,
    registration_number TEXT,
    address             TEXT NOT NULL,
    phone               TEXT,
    email               TEXT,
    logo_url            TEXT,
    -- Settings (FR-03). Tax rate is in basis points: 750 = 7.50%.
    base_currency       CHAR(3) NOT NULL DEFAULT 'NGN',
    timezone            TEXT NOT NULL DEFAULT 'Africa/Lagos',
    tax_name            TEXT NOT NULL DEFAULT 'VAT',
    tax_rate_bps        INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate_bps BETWEEN 0 AND 10000),
    prices_include_tax  BOOLEAN NOT NULL DEFAULT FALSE,
    tax_id              TEXT,
    receipt_header      TEXT,
    receipt_footer      TEXT,
    created_by          UUID NOT NULL REFERENCES users(id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by          UUID NOT NULL REFERENCES users(id),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE stores (
    id                 UUID PRIMARY KEY,
    business_id        UUID NOT NULL REFERENCES businesses(id),
    code               TEXT NOT NULL,
    name               TEXT NOT NULL,
    address            TEXT NOT NULL,
    phone              TEXT,
    email              TEXT,
    status             TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),
    -- Store settings (FR-03).
    receipt_prefix     TEXT NOT NULL DEFAULT 'RCP',
    transaction_prefix TEXT NOT NULL DEFAULT 'TXN',
    receipt_footer     TEXT,
    created_by         UUID NOT NULL REFERENCES users(id),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by         UUID NOT NULL REFERENCES users(id),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX stores_business_code_key ON stores (business_id, lower(code));
CREATE INDEX stores_business_id_idx ON stores (business_id);

-- Membership of a user in a business. business_role is set only for
-- business-wide roles (owner, administrator); store roles live in store_assignments.
CREATE TABLE business_members (
    business_id   UUID NOT NULL REFERENCES businesses(id),
    user_id       UUID NOT NULL REFERENCES users(id),
    business_role TEXT CHECK (business_role IN ('business_owner', 'administrator')),
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (business_id, user_id)
);
CREATE INDEX business_members_user_id_idx ON business_members (user_id);

-- One role per (user, store); a user may hold different roles at different stores.
CREATE TABLE store_assignments (
    store_id   UUID NOT NULL REFERENCES stores(id),
    user_id    UUID NOT NULL REFERENCES users(id),
    role       TEXT NOT NULL CHECK (role IN ('store_manager', 'cashier', 'inventory_officer', 'finance_officer')),
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (store_id, user_id)
);
CREATE INDEX store_assignments_user_id_idx ON store_assignments (user_id);

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

CREATE TABLE audit_logs (
    id            UUID PRIMARY KEY,
    business_id   UUID,
    store_id      UUID,
    actor_user_id UUID,
    action        TEXT NOT NULL,
    entity_type   TEXT NOT NULL,
    entity_id     UUID,
    outcome       TEXT NOT NULL CHECK (outcome IN ('success', 'denied', 'failure')),
    before_values JSONB,
    after_values  JSONB,
    metadata      JSONB,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_logs_business_created_idx ON audit_logs (business_id, created_at DESC);
CREATE INDEX audit_logs_entity_idx ON audit_logs (entity_id);

-- Audit entries are append-only (FR-27).
CREATE FUNCTION audit_logs_immutable() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'audit_logs is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_logs_no_update_delete
    BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION audit_logs_immutable();
