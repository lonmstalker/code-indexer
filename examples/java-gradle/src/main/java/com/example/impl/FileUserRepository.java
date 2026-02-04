package com.example.impl;

import com.example.interfaces.Repository;
import com.example.models.User;
import com.fasterxml.jackson.core.type.TypeReference;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.io.File;
import java.io.IOException;
import java.util.*;

/**
 * File-based implementation of User repository.
 */
public class FileUserRepository implements Repository<User> {

    private static final ObjectMapper MAPPER = new ObjectMapper();

    private final File file;

    public FileUserRepository(String filePath) {
        this.file = new File(filePath);
    }

    @Override
    public Optional<User> findById(Long id) {
        return loadAll().stream()
                .filter(u -> id.equals(u.id()))
                .findFirst();
    }

    @Override
    public List<User> findAll() {
        return loadAll();
    }

    @Override
    public User save(User entity) {
        List<User> users = loadAll();
        User toSave = entity.id() == null
                ? entity.withId(generateId(users))
                : entity;

        users.removeIf(u -> toSave.id().equals(u.id()));
        users.add(toSave);
        saveAll(users);
        return toSave;
    }

    @Override
    public boolean delete(Long id) {
        List<User> users = loadAll();
        boolean removed = users.removeIf(u -> id.equals(u.id()));
        if (removed) {
            saveAll(users);
        }
        return removed;
    }

    private List<User> loadAll() {
        if (!file.exists()) {
            return new ArrayList<>();
        }
        try {
            return MAPPER.readValue(file, new TypeReference<>() {});
        } catch (IOException e) {
            return new ArrayList<>();
        }
    }

    private void saveAll(List<User> users) {
        try {
            MAPPER.writeValue(file, users);
        } catch (IOException e) {
            throw new RuntimeException("Failed to save users", e);
        }
    }

    private Long generateId(List<User> users) {
        return users.stream()
                .map(User::id)
                .filter(Objects::nonNull)
                .max(Long::compareTo)
                .orElse(0L) + 1;
    }
}
