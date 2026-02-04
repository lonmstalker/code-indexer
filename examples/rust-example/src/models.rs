use serde::{Deserialize, Serialize};

use crate::traits::{Serializable, Validator};

/// Marker trait for entities with ID.
pub trait Entity {
    fn id(&self) -> u64;
    fn set_id(&mut self, id: u64);
}

/// User status enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Active,
    Inactive,
    Pending,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Inactive => "inactive",
            Status::Pending => "pending",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Status::Active),
            "inactive" => Some(Status::Inactive),
            "pending" => Some(Status::Pending),
            _ => None,
        }
    }
}

/// User entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub status: Status,
}

impl User {
    pub fn new(name: String, email: String) -> Self {
        Self {
            id: 0,
            name,
            email,
            status: Status::Pending,
        }
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    pub fn activate(&mut self) {
        self.status = Status::Active;
    }

    pub fn deactivate(&mut self) {
        self.status = Status::Inactive;
    }
}

impl Entity for User {
    fn id(&self) -> u64 {
        self.id
    }

    fn set_id(&mut self, id: u64) {
        self.id = id;
    }
}

impl Serializable for User {
    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

impl Validator for User {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.name.is_empty() {
            errors.push("Name cannot be empty".to_string());
        }

        if !self.email.contains('@') {
            errors.push("Invalid email format".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Product entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: u64,
    pub name: String,
    pub price: f64,
    pub in_stock: bool,
}

impl Product {
    pub fn new(name: String, price: f64) -> Self {
        Self {
            id: 0,
            name,
            price,
            in_stock: true,
        }
    }

    pub fn with_stock(mut self, in_stock: bool) -> Self {
        self.in_stock = in_stock;
        self
    }

    pub fn apply_discount(&mut self, percent: f64) {
        self.price *= 1.0 - (percent / 100.0);
    }
}

impl Entity for Product {
    fn id(&self) -> u64 {
        self.id
    }

    fn set_id(&mut self, id: u64) {
        self.id = id;
    }
}

impl Serializable for Product {
    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

impl Validator for Product {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.name.is_empty() {
            errors.push("Name cannot be empty".to_string());
        }

        if self.price < 0.0 {
            errors.push("Price cannot be negative".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Type alias for user list.
pub type UserList = Vec<User>;

/// Type alias for product list.
pub type ProductList = Vec<Product>;
