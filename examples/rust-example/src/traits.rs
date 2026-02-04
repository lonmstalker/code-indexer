use crate::models::Entity;

/// Repository trait for CRUD operations on entities.
pub trait Repository<T: Entity> {
    /// Find entity by ID.
    fn find_by_id(&self, id: u64) -> Option<T>;

    /// Find all entities.
    fn find_all(&self) -> Vec<T>;

    /// Save entity and return its ID.
    fn save(&mut self, entity: T) -> u64;

    /// Delete entity by ID, returns true if deleted.
    fn delete(&mut self, id: u64) -> bool;

    /// Check if entity exists by ID.
    fn exists(&self, id: u64) -> bool {
        self.find_by_id(id).is_some()
    }

    /// Count all entities.
    fn count(&self) -> usize {
        self.find_all().len()
    }
}

/// Serializable trait for JSON conversion.
pub trait Serializable {
    fn to_json(&self) -> String;
    fn from_json(json: &str) -> Result<Self, String>
    where
        Self: Sized;
}

/// Validator trait for entity validation.
pub trait Validator {
    fn validate(&self) -> Result<(), Vec<String>>;

    fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}
