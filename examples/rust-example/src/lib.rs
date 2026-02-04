pub mod implementations;
pub mod models;
pub mod traits;

use implementations::{InMemoryProductRepository, InMemoryUserRepository};
use models::{Product, Status, User};
use traits::{Repository, Serializable, Validator};

/// Demonstrate Rust closures and lambdas.
pub fn demonstrate_closures() {
    let mut user_repo = InMemoryUserRepository::new();

    // Create users with builder pattern
    let users = vec![
        User::new("Alice".to_string(), "alice@example.com".to_string())
            .with_status(Status::Active),
        User::new("Bob".to_string(), "bob@example.com".to_string()).with_status(Status::Active),
        User::new("Charlie".to_string(), "charlie@example.com".to_string())
            .with_status(Status::Inactive),
    ];

    // Save using for_each with closure
    users.into_iter().for_each(|user| {
        user_repo.save(user);
    });

    // Filter using closure
    let active_users: Vec<_> = user_repo
        .find_all()
        .into_iter()
        .filter(|u| u.status == Status::Active)
        .collect();

    println!("Active users: {}", active_users.len());

    // Map using closure
    let user_names: Vec<String> = user_repo
        .find_all()
        .into_iter()
        .map(|u| u.name.clone())
        .collect();

    println!("User names: {:?}", user_names);

    // Find using closure
    let alice = user_repo
        .find_all()
        .into_iter()
        .find(|u| u.name == "Alice");

    if let Some(user) = alice {
        println!("Found: {}", user.to_json());
    }
}

/// Demonstrate validation.
pub fn demonstrate_validation() {
    let valid_user = User::new("Test".to_string(), "test@example.com".to_string());
    let invalid_user = User::new("".to_string(), "invalid-email".to_string());

    println!("Valid user is_valid: {}", valid_user.is_valid());
    println!("Invalid user is_valid: {}", invalid_user.is_valid());

    if let Err(errors) = invalid_user.validate() {
        println!("Validation errors: {:?}", errors);
    }
}

/// Demonstrate products with price calculations.
pub fn demonstrate_products() {
    let mut product_repo = InMemoryProductRepository::new();

    // Create products
    let products = vec![
        Product::new("Laptop".to_string(), 999.99),
        Product::new("Mouse".to_string(), 29.99).with_stock(true),
        Product::new("Keyboard".to_string(), 79.99).with_stock(false),
    ];

    // Save all products
    for product in products {
        product_repo.save(product);
    }

    // Find products in price range using closure
    let affordable: Vec<_> = product_repo
        .find_all()
        .into_iter()
        .filter(|p| p.price < 100.0)
        .collect();

    println!("Affordable products: {}", affordable.len());

    // Calculate total price using fold
    let total: f64 = product_repo
        .find_all()
        .iter()
        .map(|p| p.price)
        .fold(0.0, |acc, price| acc + price);

    println!("Total price: {:.2}", total);

    // Apply discount to all products
    let discounted: Vec<_> = product_repo
        .find_all()
        .into_iter()
        .map(|mut p| {
            p.apply_discount(10.0);
            p
        })
        .collect();

    println!("Discounted products: {:?}", discounted);
}

/// Higher-order function example.
pub fn process_users<F>(users: Vec<User>, processor: F) -> Vec<User>
where
    F: Fn(User) -> User,
{
    users.into_iter().map(processor).collect()
}

/// Generic function with trait bounds.
pub fn save_and_validate<T>(repo: &mut impl Repository<T>, entity: T) -> Result<u64, Vec<String>>
where
    T: models::Entity + Validator,
{
    entity.validate()?;
    Ok(repo.save(entity))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::new("Test".to_string(), "test@example.com".to_string());
        assert_eq!(user.name, "Test");
        assert_eq!(user.status, Status::Pending);
    }

    #[test]
    fn test_repository_operations() {
        let mut repo = InMemoryUserRepository::new();
        let user = User::new("Test".to_string(), "test@example.com".to_string());

        let id = repo.save(user);
        assert!(repo.exists(id));

        let found = repo.find_by_id(id);
        assert!(found.is_some());

        assert!(repo.delete(id));
        assert!(!repo.exists(id));
    }

    #[test]
    fn test_serialization() {
        let user = User::new("Test".to_string(), "test@example.com".to_string());
        let json = user.to_json();
        let restored = User::from_json(&json).unwrap();

        assert_eq!(user.name, restored.name);
        assert_eq!(user.email, restored.email);
    }
}
