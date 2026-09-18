#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockTransfer {
    pub id: u64,
    pub from_store_id: u64,
    pub to_store_id: u64,
    pub product_id: u64,
    pub quantity: u32,
    pub status: TransferStatus,
    pub requested_by: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferStatus {
    Pending,
    Approved,
    Rejected,
    Completed,
    Cancelled,
}