use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use crate::models::{Entity, Product, User};
use crate::traits::{Repository, Serializable};

/// In-memory repository for users.
pub struct InMemoryUserRepository {
    storage: HashMap<u64, User>,
    next_id: u64,
}

impl InMemoryUserRepository {
    pub fn new() -> Self {
        Self {
            storage: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn find_by_email(&self, email: &str) -> Option<&User> {
        self.storage.values().find(|u| u.email == email)
    }

    pub fn find_active(&self) -> Vec<User> {
        self.storage
            .values()
            .filter(|u| u.status == crate::models::Status::Active)
            .cloned()
            .collect()
    }
}

impl Default for InMemoryUserRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Repository<User> for InMemoryUserRepository {
    fn find_by_id(&self, id: u64) -> Option<User> {
        self.storage.get(&id).cloned()
    }

    fn find_all(&self) -> Vec<User> {
        self.storage.values().cloned().collect()
    }

    fn save(&mut self, mut entity: User) -> u64 {
        if entity.id() == 0 {
            entity.set_id(self.next_id);
            self.next_id += 1;
        }
        let id = entity.id();
        self.storage.insert(id, entity);
        id
    }

    fn delete(&mut self, id: u64) -> bool {
        self.storage.remove(&id).is_some()
    }
}

/// File-based repository for users.
pub struct FileUserRepository {
    file_path: PathBuf,
}

impl FileUserRepository {
    pub fn new(file_path: PathBuf) -> Self {
        Self { file_path }
    }

    fn load_all(&self) -> HashMap<u64, User> {
        if let Ok(file) = File::open(&self.file_path) {
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    fn save_all(&self, data: &HashMap<u64, User>) {
        if let Ok(file) = File::create(&self.file_path) {
            let writer = BufWriter::new(file);
            let _ = serde_json::to_writer(writer, data);
        }
    }

    fn next_id(&self, data: &HashMap<u64, User>) -> u64 {
        data.keys().max().map(|m| m + 1).unwrap_or(1)
    }
}

impl Repository<User> for FileUserRepository {
    fn find_by_id(&self, id: u64) -> Option<User> {
        self.load_all().get(&id).cloned()
    }

    fn find_all(&self) -> Vec<User> {
        self.load_all().values().cloned().collect()
    }

    fn save(&mut self, mut entity: User) -> u64 {
        let mut data = self.load_all();
        if entity.id() == 0 {
            entity.set_id(self.next_id(&data));
        }
        let id = entity.id();
        data.insert(id, entity);
        self.save_all(&data);
        id
    }

    fn delete(&mut self, id: u64) -> bool {
        let mut data = self.load_all();
        let removed = data.remove(&id).is_some();
        if removed {
            self.save_all(&data);
        }
        removed
    }
}

/// In-memory repository for products.
pub struct InMemoryProductRepository {
    storage: HashMap<u64, Product>,
    next_id: u64,
}

impl InMemoryProductRepository {
    pub fn new() -> Self {
        Self {
            storage: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn find_in_stock(&self) -> Vec<Product> {
        self.storage
            .values()
            .filter(|p| p.in_stock)
            .cloned()
            .collect()
    }

    pub fn find_by_price_range(&self, min: f64, max: f64) -> Vec<Product> {
        self.storage
            .values()
            .filter(|p| p.price >= min && p.price <= max)
            .cloned()
            .collect()
    }
}

impl Default for InMemoryProductRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Repository<Product> for InMemoryProductRepository {
    fn find_by_id(&self, id: u64) -> Option<Product> {
        self.storage.get(&id).cloned()
    }

    fn find_all(&self) -> Vec<Product> {
        self.storage.values().cloned().collect()
    }

    fn save(&mut self, mut entity: Product) -> u64 {
        if entity.id() == 0 {
            entity.set_id(self.next_id);
            self.next_id += 1;
        }
        let id = entity.id();
        self.storage.insert(id, entity);
        id
    }

    fn delete(&mut self, id: u64) -> bool {
        self.storage.remove(&id).is_some()
    }
}

/// File-based repository for products.
pub struct FileProductRepository {
    file_path: PathBuf,
}

impl FileProductRepository {
    pub fn new(file_path: PathBuf) -> Self {
        Self { file_path }
    }

    fn load_all(&self) -> HashMap<u64, Product> {
        if let Ok(file) = File::open(&self.file_path) {
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    fn save_all(&self, data: &HashMap<u64, Product>) {
        if let Ok(file) = File::create(&self.file_path) {
            let writer = BufWriter::new(file);
            let _ = serde_json::to_writer(writer, data);
        }
    }

    fn next_id(&self, data: &HashMap<u64, Product>) -> u64 {
        data.keys().max().map(|m| m + 1).unwrap_or(1)
    }
}

impl Repository<Product> for FileProductRepository {
    fn find_by_id(&self, id: u64) -> Option<Product> {
        self.load_all().get(&id).cloned()
    }

    fn find_all(&self) -> Vec<Product> {
        self.load_all().values().cloned().collect()
    }

    fn save(&mut self, mut entity: Product) -> u64 {
        let mut data = self.load_all();
        if entity.id() == 0 {
            entity.set_id(self.next_id(&data));
        }
        let id = entity.id();
        data.insert(id, entity);
        self.save_all(&data);
        id
    }

    fn delete(&mut self, id: u64) -> bool {
        let mut data = self.load_all();
        let removed = data.remove(&id).is_some();
        if removed {
            self.save_all(&data);
        }
        removed
    }
}
