#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub id: u64,
    pub store_id: u64,
    pub product_id: u64,
    pub quantity: u32,
    pub reorder_level: u32,
}