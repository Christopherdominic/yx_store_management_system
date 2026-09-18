#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    pub id: u64,
    pub store_id: u64,
    pub description: String,
    pub amount: f64,
    pub category: ExpenseCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpenseCategory {
    Electricity,
    Rent,
    Transport,
    Maintenance,
    Salary,
    Other,
}