package com.example.impl;

import com.example.interfaces.Repository;
import com.example.models.Product;

import java.math.BigDecimal;
import java.util.*;
import java.util.concurrent.atomic.AtomicLong;

/**
 * In-memory implementation of Product repository.
 */
public class InMemoryProductRepository implements Repository<Product> {

    private final Map<Long, Product> storage = new HashMap<>();
    private final AtomicLong idGenerator = new AtomicLong(1);

    @Override
    public Optional<Product> findById(Long id) {
        return Optional.ofNullable(storage.get(id));
    }

    @Override
    public List<Product> findAll() {
        return new ArrayList<>(storage.values());
    }

    @Override
    public Product save(Product entity) {
        Product toSave = entity.id() == null
                ? entity.withId(idGenerator.getAndIncrement())
                : entity;
        storage.put(toSave.id(), toSave);
        return toSave;
    }

    @Override
    public boolean delete(Long id) {
        return storage.remove(id) != null;
    }

    public List<Product> findInStock() {
        return storage.values().stream()
                .filter(Product::inStock)
                .toList();
    }

    public List<Product> findByPriceRange(BigDecimal min, BigDecimal max) {
        return storage.values().stream()
                .filter(p -> p.price().compareTo(min) >= 0 && p.price().compareTo(max) <= 0)
                .toList();
    }

    public List<Product> findByNameContaining(String namePart) {
        return storage.values().stream()
                .filter(p -> p.name().toLowerCase().contains(namePart.toLowerCase()))
                .toList();
    }

    public BigDecimal calculateTotalValue() {
        return storage.values().stream()
                .map(Product::price)
                .reduce(BigDecimal.ZERO, BigDecimal::add);
    }
}
