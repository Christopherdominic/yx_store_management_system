//! End-to-end tests for product management (FR-07 – FR-09).

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

use common::{Amiable, TestApp, amiable, error_fields};

fn products(a: &Amiable) -> String {
    format!("/businesses/{}/products", a.business)
}

fn categories(a: &Amiable) -> String {
    format!("/businesses/{}/product-categories", a.business)
}

fn rice() -> Value {
    json!({
        "name": "Rice 50kg",
        "sku": "rice-50",
        "barcode": "6151234567890",
        "brand": "Mama Gold",
        "unit_of_measure": "bag",
        "cost_price": "65000.00",
        "selling_price": "72500.50",
        "attributes": { "origin": "Nigeria" },
    })
}

async fn create(app: &TestApp, a: &Amiable, body: Value) -> Value {
    let (status, product) = app.post(&products(a), &a.owner, body).await;
    assert_eq!(status, StatusCode::CREATED, "{product}");
    product
}

// FR-07: create, update, search and archive; history stays readable.
#[sqlx::test]
async fn product_catalogue_lifecycle(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;

    let product = create(&app, &a, rice()).await;
    let id = product["id"].as_str().unwrap();
    let url = format!("{}/{id}", products(&a));
    assert_eq!(product["sku"], "RICE-50", "SKUs are uppercased");
    assert_eq!(
        product["selling_price"], "72500.50",
        "prices are exact strings"
    );
    assert_eq!(product["status"], "active");

    // Update descriptive fields; `""` clears an optional one.
    let (status, body) = app
        .patch(
            &url,
            &a.owner,
            json!({ "name": "Rice 50kg (Long grain)", "brand": "" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Rice 50kg (Long grain)");
    assert!(body["brand"].is_null());

    // Prices can't be changed through PATCH.
    let (status, _) = app
        .patch(&url, &a.owner, json!({ "selling_price": "1.00" }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Search by name, SKU and barcode.
    for q in ["long%20grain", "rice-5", "615123"] {
        let (status, found) = app.get(&format!("{}?q={q}", products(&a)), &a.owner).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(found.as_array().unwrap().len(), 1, "search {q}");
    }

    // Archive: still readable, filtered out of active listings.
    let (status, body) = app
        .post(&format!("{url}/archive"), &a.owner, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "archived");
    let (status, body) = app.get(&url, &a.owner).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "archived");
    let (_, active) = app
        .get(&format!("{}?status=active", products(&a)), &a.owner)
        .await;
    assert!(active.as_array().unwrap().is_empty());

    let (status, body) = app
        .post(&format!("{url}/restore"), &a.owner, json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "active");

    let (_, logs) = app
        .get(
            &format!("/businesses/{}/audit-logs?record_id={id}", a.business),
            &a.owner,
        )
        .await;
    let actions: Vec<&str> = logs
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["reactivated", "archived", "updated", "created"]);
}

// FR-07: required fields, exact prices and duplicate SKUs.
#[sqlx::test]
async fn products_are_validated(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;

    let (status, body) = app
        .post(
            &products(&a),
            &a.owner,
            json!({
                "name": " ",
                "sku": "has space",
                "cost_price": "-1",
                "selling_price": "10.005",
                "image_url": "ftp://x",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        error_fields(&body),
        ["name", "sku", "image_url", "cost_price", "selling_price"]
    );

    create(&app, &a, rice()).await;
    let mut duplicate = rice();
    duplicate["sku"] = json!("RICE-50");
    duplicate["barcode"] = json!(null);
    let (status, body) = app.post(&products(&a), &a.owner, duplicate).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["error"]["message"].as_str().unwrap().contains("SKU"));

    let mut same_barcode = rice();
    same_barcode["sku"] = json!("RICE-25");
    let (status, body) = app.post(&products(&a), &a.owner, same_barcode).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("barcode")
    );
}

// FR-08: two variants are identified, looked up and priced separately.
#[sqlx::test]
async fn variants_are_identified_separately(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let shirt = create(
        &app,
        &a,
        json!({ "name": "Polo shirt", "sku": "POLO", "cost_price": "3000", "selling_price": "4500" }),
    )
    .await;
    let variants = format!(
        "{}/{}/variants",
        products(&a),
        shirt["id"].as_str().unwrap()
    );

    let (status, large) = app
        .post(
            &variants,
            &a.owner,
            json!({
                "name": "Large / Red", "sku": "POLO-L-RED", "barcode": "1111",
                "attributes": { "size": "L", "colour": "Red" }, "selling_price": "5000",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{large}");
    assert_eq!(large["selling_price"], "5000.00");
    assert_eq!(
        large["cost_price"], "3000.00",
        "defaults to the product's price"
    );

    let (status, _) = app
        .post(
            &variants,
            &a.owner,
            json!({ "name": "Small / Red", "sku": "POLO-S-RED", "barcode": "2222" }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    // SKUs are unique across products and variants.
    let (status, _) = app
        .post(
            &variants,
            &a.owner,
            json!({ "name": "Clash", "sku": "polo" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (_, list) = app.get(&variants, &a.owner).await;
    assert_eq!(list.as_array().unwrap().len(), 2);

    // Barcode lookup finds the variant; SKU lookup finds the product.
    let lookup = |code: &str| format!("{}/lookup?code={code}", products(&a));
    let (status, found) = app.get(&lookup("1111"), &a.owner).await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["variant"]["sku"], "POLO-L-RED");
    assert_eq!(found["product"]["sku"], "POLO");
    let (_, found) = app.get(&lookup("polo"), &a.owner).await;
    assert!(found["variant"].is_null());
    let (status, _) = app.get(&lookup("9999"), &a.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Searching by a variant SKU finds the parent product.
    let (_, found) = app
        .get(&format!("{}?q=POLO-S", products(&a)), &a.owner)
        .await;
    assert_eq!(found.as_array().unwrap().len(), 1);

    // Changing a variant's barcode frees the old one.
    let large_url = format!("{variants}/{}", large["id"].as_str().unwrap());
    let (status, _) = app
        .patch(&large_url, &a.owner, json!({ "barcode": "3333" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.get(&lookup("1111"), &a.owner).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // An archived product can't get new variants.
    let product_url = format!("{}/{}", products(&a), shirt["id"].as_str().unwrap());
    app.post(&format!("{product_url}/archive"), &a.owner, json!({}))
        .await;
    let (status, _) = app
        .post(
            &variants,
            &a.owner,
            json!({ "name": "XL", "sku": "POLO-XL" }),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

// FR-08: nested categories with archive rules.
#[sqlx::test]
async fn categories_nest_and_archive(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;

    let (status, groceries) = app
        .post(&categories(&a), &a.owner, json!({ "name": "Groceries" }))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{groceries}");
    let groceries_id = groceries["id"].as_str().unwrap();
    let (status, grains) = app
        .post(
            &categories(&a),
            &a.owner,
            json!({ "name": "Grains", "parent_id": groceries_id }),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{grains}");
    let grains_id = grains["id"].as_str().unwrap();

    // Names are unique among siblings, ignoring case.
    let (status, _) = app
        .post(&categories(&a), &a.owner, json!({ "name": "groceries" }))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // A category can't be moved under its own subcategory.
    let (status, body) = app
        .patch(
            &format!("{}/{groceries_id}", categories(&a)),
            &a.owner,
            json!({ "parent_id": grains_id }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (_, children) = app
        .get(
            &format!("{}?parent_id={groceries_id}", categories(&a)),
            &a.owner,
        )
        .await;
    assert_eq!(children.as_array().unwrap().len(), 1);

    // Products can be filed under a category; archived ones take no new products.
    let mut product = rice();
    product["category_id"] = json!(grains_id);
    let rice = create(&app, &a, product).await;
    let (_, in_grains) = app
        .get(
            &format!("{}?category_id={grains_id}", products(&a)),
            &a.owner,
        )
        .await;
    assert_eq!(in_grains.as_array().unwrap().len(), 1);

    app.post(
        &format!("{}/{grains_id}/archive", categories(&a)),
        &a.owner,
        json!({}),
    )
    .await;
    let (status, body) = app
        .post(
            &products(&a),
            &a.owner,
            json!({
                "name": "Beans", "sku": "BEANS", "category_id": grains_id,
                "cost_price": "1", "selling_price": "2",
            }),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(error_fields(&body), ["category_id"]);

    // Existing products keep their category.
    let (_, body) = app
        .get(
            &format!("{}/{}", products(&a), rice["id"].as_str().unwrap()),
            &a.owner,
        )
        .await;
    assert_eq!(body["category_id"], grains_id);
}

// FR-09: price changes are recorded with previous and new values.
#[sqlx::test]
async fn price_changes_are_recorded(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let product = create(&app, &a, rice()).await;
    let id = product["id"].as_str().unwrap();
    let url = format!("{}/{id}", products(&a));

    let (status, body) = app
        .patch(
            &format!("{url}/prices"),
            &a.owner,
            json!({ "selling_price": "75000", "reason": "Supplier increase" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["selling_price"], "75000.00");
    assert_eq!(body["cost_price"], "65000.00", "unchanged");

    // Both prices: two entries. The same price again: nothing recorded.
    app.patch(
        &format!("{url}/prices"),
        &a.owner,
        json!({ "cost_price": "66000", "selling_price": "76000" }),
    )
    .await;
    let (status, _) = app
        .patch(
            &format!("{url}/prices"),
            &a.owner,
            json!({ "cost_price": "66000.00" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .patch(&format!("{url}/prices"), &a.owner, json!({}))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, history) = app.get(&format!("{url}/price-history"), &a.owner).await;
    assert_eq!(status, StatusCode::OK);
    let history = history.as_array().unwrap();
    assert_eq!(history.len(), 3, "{history:?}");
    let first = &history[2];
    assert_eq!(first["price_type"], "selling");
    assert_eq!(first["previous_price"], "72500.50");
    assert_eq!(first["new_price"], "75000.00");
    assert_eq!(first["reason"], "Supplier increase");
    assert!(first["changed_by"].is_string());

    let (_, selling) = app
        .get(&format!("{url}/price-history?price_type=selling"), &a.owner)
        .await;
    assert_eq!(selling.as_array().unwrap().len(), 2);

    // Variant prices have their own history entries.
    let (_, variant) = app
        .post(
            &format!("{url}/variants"),
            &a.owner,
            json!({ "name": "25kg", "sku": "RICE-25" }),
        )
        .await;
    let variant_id = variant["id"].as_str().unwrap();
    let (status, body) = app
        .patch(
            &format!("{url}/variants/{variant_id}/prices"),
            &a.owner,
            json!({ "selling_price": "39000" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, entries) = app
        .get(
            &format!("{url}/price-history?variant_id={variant_id}"),
            &a.owner,
        )
        .await;
    let entries = entries.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["previous_price"], "76000.00");

    let (_, logs) = app
        .get(
            &format!("/businesses/{}/audit-logs?action=price_changed", a.business),
            &a.owner,
        )
        .await;
    assert_eq!(logs.as_array().unwrap().len(), 3);
}

// FR-06, FR-09: who may view, edit and reprice the catalogue.
#[sqlx::test]
async fn catalogue_permissions_follow_roles(pool: PgPool) {
    let app = TestApp::new(pool);
    let a = amiable(&app).await;
    let product = create(&app, &a, rice()).await;
    let url = format!("{}/{}", products(&a), product["id"].as_str().unwrap());

    for (email, role) in [
        ("cashier@amiable.ng", "cashier"),
        ("stock@amiable.ng", "inventory_officer"),
        ("manager@amiable.ng", "store_manager"),
    ] {
        app.employee(
            &a.owner,
            &a.business,
            email,
            json!({ "stores": [{ "store_id": a.lagos, "role": role }] }),
        )
        .await;
    }
    let cashier = app.token("cashier@amiable.ng").await;
    let stock = app.token("stock@amiable.ng").await;
    let manager = app.token("manager@amiable.ng").await;
    let new_product =
        |sku: &str| json!({ "name": sku, "sku": sku, "cost_price": "1", "selling_price": "2" });
    let reprice = json!({ "selling_price": "80000" });

    // Cashier: view only.
    let (status, _) = app.get(&url, &cashier).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.post(&products(&a), &cashier, new_product("C1")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Inventory officer: manage products, but not prices.
    let (status, _) = app.post(&products(&a), &stock, new_product("S1")).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = app
        .patch(&format!("{url}/prices"), &stock, reprice.clone())
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Store manager: may change prices.
    let (status, _) = app.patch(&format!("{url}/prices"), &manager, reprice).await;
    assert_eq!(status, StatusCode::OK);

    // Another business can't see or use this catalogue.
    let (other, other_business) = app.register("owner@other.ng", "Other").await;
    let (status, _) = app.get(&url, &other).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .get(
            &format!(
                "/businesses/{other_business}/products/{}",
                product["id"].as_str().unwrap()
            ),
            &other,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
