package com.example;

import com.example.impl.InMemoryProductRepository;
import com.example.impl.InMemoryUserRepository;
import com.example.models.Product;
import com.example.models.Status;
import com.example.models.User;

import java.math.BigDecimal;
import java.util.List;
import java.util.Optional;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.stream.Collectors;

/**
 * Main class demonstrating Java features.
 */
public class Main {

    public static void main(String[] args) {
        demonstrateLambdas();
        demonstrateMethodReferences();
        demonstrateStreamApi();
        demonstrateOptional();
    }

    /**
     * Demonstrate lambda expressions.
     */
    public static void demonstrateLambdas() {
        var userRepo = new InMemoryUserRepository();

        // Create users
        List.of(
                new User("Alice", "alice@example.com"),
                new User("Bob", "bob@example.com"),
                new User("Charlie", "charlie@example.com")
        ).forEach(user -> userRepo.save(user.activate()));

        // Filter with lambda
        Predicate<User> isActive = u -> u.status() == Status.ACTIVE;
        List<User> activeUsers = userRepo.findAll().stream()
                .filter(isActive)
                .collect(Collectors.toList());

        System.out.println("Active users: " + activeUsers.size());

        // Map with lambda
        Function<User, String> toName = u -> u.name();
        List<String> names = userRepo.findAll().stream()
                .map(toName)
                .collect(Collectors.toList());

        System.out.println("User names: " + names);
    }

    /**
     * Demonstrate method references.
     */
    public static void demonstrateMethodReferences() {
        var userRepo = new InMemoryUserRepository();

        userRepo.save(new User("Diana", "diana@example.com"));
        userRepo.save(new User("Eve", "eve@example.com"));

        // Method reference: instance method
        List<String> names = userRepo.findAll().stream()
                .map(User::name)
                .collect(Collectors.toList());

        System.out.println("Names via method reference: " + names);

        // Method reference: static method
        List<String> jsons = userRepo.findAll().stream()
                .map(User::toJson)
                .collect(Collectors.toList());

        System.out.println("JSON representations: " + jsons.size());
    }

    /**
     * Demonstrate Stream API.
     */
    public static void demonstrateStreamApi() {
        var productRepo = new InMemoryProductRepository();

        // Save products
        productRepo.save(new Product("Laptop", 999.99));
        productRepo.save(new Product("Mouse", 29.99));
        productRepo.save(new Product("Keyboard", 79.99));
        productRepo.save(new Product("Monitor", 299.99));

        // Chain operations
        List<Product> affordable = productRepo.findAll().stream()
                .filter(p -> p.price().compareTo(BigDecimal.valueOf(100)) < 0)
                .sorted((a, b) -> a.price().compareTo(b.price()))
                .collect(Collectors.toList());

        System.out.println("Affordable products: " + affordable.size());

        // Reduce
        BigDecimal total = productRepo.findAll().stream()
                .map(Product::price)
                .reduce(BigDecimal.ZERO, BigDecimal::add);

        System.out.println("Total price: " + total);

        // GroupingBy
        var byPriceRange = productRepo.findAll().stream()
                .collect(Collectors.groupingBy(p ->
                        p.price().compareTo(BigDecimal.valueOf(100)) < 0 ? "cheap" : "expensive"
                ));

        System.out.println("Grouped: " + byPriceRange);
    }

    /**
     * Demonstrate Optional usage.
     */
    public static void demonstrateOptional() {
        var userRepo = new InMemoryUserRepository();

        User saved = userRepo.save(new User("Frank", "frank@example.com"));

        // Optional operations
        Optional<User> found = userRepo.findById(saved.id());

        found.ifPresent(u -> System.out.println("Found user: " + u.name()));

        String name = found.map(User::name).orElse("Unknown");
        System.out.println("User name: " + name);

        // OrElseThrow
        User user = userRepo.findById(saved.id())
                .orElseThrow(() -> new RuntimeException("User not found"));

        System.out.println("User found: " + user.name());

        // Filter in Optional
        Optional<User> activeUser = userRepo.findById(saved.id())
                .filter(User::isActive);

        System.out.println("Is active: " + activeUser.isPresent());
    }
}
