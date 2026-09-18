#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: u64,
    pub business_id: u64,
    pub name: String,
    pub phone: String,
    pub email: Option<String>,
}