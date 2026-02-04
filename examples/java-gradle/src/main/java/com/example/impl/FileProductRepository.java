package com.example.impl;

import com.example.interfaces.Repository;
import com.example.models.Product;
import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.io.File;
import java.io.IOException;
import java.util.*;

/**
 * File-based implementation of Product repository.
 */
public class FileProductRepository implements Repository<Product> {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    private final File file;

    public FileProductRepository(String filePath) {
        this.file = new File(filePath);
    }

    @Override
    public Optional<Product> findById(Long id) {
        return loadAll().stream()
                .filter(p -> id.equals(p.id()))
                .findFirst();
    }

    @Override
    public List<Product> findAll() {
        return loadAll();
    }

    @Override
    public Product save(Product entity) {
        List<Product> products = loadAll();
        Product toSave = entity.id() == null
                ? entity.withId(generateId(products))
                : entity;

        products.removeIf(p -> toSave.id().equals(p.id()));
        products.add(toSave);
        saveAll(products);
        return toSave;
    }

    @Override
    public boolean delete(Long id) {
        List<Product> products = loadAll();
        boolean removed = products.removeIf(p -> id.equals(p.id()));
        if (removed) {
            saveAll(products);
        }
        return removed;
    }

    private List<Product> loadAll() {
        if (!file.exists()) {
            return new ArrayList<>();
        }
        try {
            return MAPPER.readValue(file, new TypeReference<>() {});
        } catch (IOException e) {
            return new ArrayList<>();
        }
    }

    private void saveAll(List<Product> products) {
        try {
            MAPPER.writeValue(file, products);
        } catch (IOException e) {
            throw new RuntimeException("Failed to save products", e);
        }
    }

    private Long generateId(List<Product> products) {
        return products.stream()
                .map(Product::id)
                .filter(Objects::nonNull)
                .max(Long::compareTo)
                .orElse(0L) + 1;
    }
}
