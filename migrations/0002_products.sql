-- FR-07 – FR-09: product catalogue, categories, variants and price history.
-- Table shapes follow the models in src/models. Prices are exact NUMERIC
-- values in the business currency (NFR-30).

CREATE TABLE product_categories (
    id          UUID PRIMARY KEY,
    business_id UUID NOT NULL REFERENCES businesses(id),
    parent_id   UUID REFERENCES product_categories(id),
    name        TEXT NOT NULL,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (parent_id IS NULL OR parent_id <> id)
);
-- Names are unique among siblings; top-level categories share one parent.
CREATE UNIQUE INDEX product_categories_sibling_name_key ON product_categories (
    business_id,
    COALESCE(parent_id, '00000000-0000-0000-0000-000000000000'::uuid),
    lower(name)
);
CREATE INDEX product_categories_business_id_idx ON product_categories (business_id);

CREATE TABLE products (
    id              UUID PRIMARY KEY,
    business_id     UUID NOT NULL REFERENCES businesses(id),
    category_id     UUID REFERENCES product_categories(id),
    name            TEXT NOT NULL,
    description     TEXT,
    sku             TEXT NOT NULL,
    barcode         TEXT,
    brand           TEXT,
    image_url       TEXT,
    unit_of_measure TEXT NOT NULL CHECK (unit_of_measure IN ('piece', 'pack', 'carton', 'bag',
                                                             'kilogram', 'gram', 'litre',
                                                             'millilitre', 'metre')),
    cost_price      NUMERIC(14, 2) NOT NULL CHECK (cost_price >= 0),
    selling_price   NUMERIC(14, 2) NOT NULL CHECK (selling_price >= 0),
    attributes      JSONB NOT NULL DEFAULT '{}',
    tracks_batches  BOOLEAN NOT NULL DEFAULT FALSE,
    status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX products_business_id_idx ON products (business_id);
CREATE INDEX products_category_id_idx ON products (category_id);

CREATE TABLE product_variants (
    id            UUID PRIMARY KEY,
    business_id   UUID NOT NULL REFERENCES businesses(id),
    product_id    UUID NOT NULL REFERENCES products(id),
    name          TEXT NOT NULL,
    sku           TEXT NOT NULL,
    barcode       TEXT,
    attributes    JSONB NOT NULL DEFAULT '{}',
    cost_price    NUMERIC(14, 2) NOT NULL CHECK (cost_price >= 0),
    selling_price NUMERIC(14, 2) NOT NULL CHECK (selling_price >= 0),
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX product_variants_product_id_idx ON product_variants (product_id);

-- SKUs and barcodes must be unique across products AND variants within a
-- business (FR-07, FR-08). Both tables register their codes here, in the
-- same transaction, so concurrent requests cannot create duplicates.
-- `code` is stored uppercased so matching ignores case.
CREATE TABLE catalog_codes (
    business_id UUID NOT NULL REFERENCES businesses(id),
    kind        TEXT NOT NULL CHECK (kind IN ('sku', 'barcode')),
    code        TEXT NOT NULL,
    product_id  UUID NOT NULL REFERENCES products(id),
    variant_id  UUID REFERENCES product_variants(id)
);
CREATE UNIQUE INDEX catalog_codes_sku_key ON catalog_codes (business_id, code) WHERE kind = 'sku';
CREATE UNIQUE INDEX catalog_codes_barcode_key ON catalog_codes (business_id, code) WHERE kind = 'barcode';
CREATE INDEX catalog_codes_product_id_idx ON catalog_codes (product_id);

-- FR-09. One row per changed price; append-only.
CREATE TABLE product_price_history (
    id             UUID PRIMARY KEY,
    business_id    UUID NOT NULL REFERENCES businesses(id),
    product_id     UUID NOT NULL REFERENCES products(id),
    variant_id     UUID REFERENCES product_variants(id),
    price_type     TEXT NOT NULL CHECK (price_type IN ('cost', 'selling')),
    previous_price NUMERIC(14, 2) NOT NULL,
    new_price      NUMERIC(14, 2) NOT NULL,
    effective_at   TIMESTAMPTZ NOT NULL,
    changed_by     UUID NOT NULL REFERENCES users(id),
    approved_by    UUID REFERENCES users(id),
    reason         TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX product_price_history_product_idx
    ON product_price_history (product_id, created_at DESC);

CREATE FUNCTION product_price_history_immutable() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'product_price_history is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER product_price_history_no_update_delete
    BEFORE UPDATE OR DELETE ON product_price_history
    FOR EACH ROW EXECUTE FUNCTION product_price_history_immutable();
