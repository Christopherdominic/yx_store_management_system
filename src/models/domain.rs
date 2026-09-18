use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Business {
    pub id: u64,
    pub name: String,
    pub stores: Vec<Store>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Store {
    pub id: u64,
    pub business_id: u64,
    pub name: String,
    pub address: String,
    pub status: StoreStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StoreStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreEmployee {
    pub user_id: u64,
    pub store_id: u64,
    pub role: StoreRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StoreRole {
    Manager,
    Cashier,
    InventoryOfficer,
}