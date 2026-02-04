package com.example.impl;

import com.example.interfaces.Repository;
import com.example.models.Status;
import com.example.models.User;

import java.util.*;
import java.util.concurrent.atomic.AtomicLong;

/**
 * In-memory implementation of User repository.
 */
public class InMemoryUserRepository implements Repository<User> {

    private final Map<Long, User> storage = new HashMap<>();
    private final AtomicLong idGenerator = new AtomicLong(1);

    @Override
    public Optional<User> findById(Long id) {
        return Optional.ofNullable(storage.get(id));
    }

    @Override
    public List<User> findAll() {
        return new ArrayList<>(storage.values());
    }

    @Override
    public User save(User entity) {
        User toSave = entity.id() == null
                ? entity.withId(idGenerator.getAndIncrement())
                : entity;
        storage.put(toSave.id(), toSave);
        return toSave;
    }

    @Override
    public boolean delete(Long id) {
        return storage.remove(id) != null;
    }

    public Optional<User> findByEmail(String email) {
        return storage.values().stream()
                .filter(u -> u.email().equals(email))
                .findFirst();
    }

    public List<User> findByStatus(Status status) {
        return storage.values().stream()
                .filter(u -> u.status() == status)
                .toList();
    }

    public List<User> findActive() {
        return findByStatus(Status.ACTIVE);
    }

    public List<User> findByNameContaining(String namePart) {
        return storage.values().stream()
                .filter(u -> u.name().toLowerCase().contains(namePart.toLowerCase()))
                .toList();
    }
}
