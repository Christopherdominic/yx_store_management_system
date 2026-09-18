#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: u64,
    pub business_id: u64,
    pub name: String,
    pub sku: String,
    pub price: f64,
}