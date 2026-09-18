#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sale {
    pub id: u64,
    pub store_id: u64,
    pub cashier_id: u64,
    pub total: f64,
    pub payment_method: PaymentMethod,
    pub status: SaleStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleItem {
    pub id: u64,
    pub sale_id: u64,
    pub product_id: u64,
    pub quantity: u32,
    pub unit_price: f64,
}