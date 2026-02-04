package com.example.models;

import com.example.abstracts.AbstractEntity;
import com.example.interfaces.Validator;

import java.util.ArrayList;
import java.util.List;

/**
 * User entity record.
 */
public record User(Long id, String name, String email, Status status) implements Validator {

    public User {
        if (status == null) {
            status = Status.PENDING;
        }
    }

    public User(String name, String email) {
        this(null, name, email, Status.PENDING);
    }

    public User withId(Long id) {
        return new User(id, name, email, status);
    }

    public User withStatus(Status status) {
        return new User(id, name, email, status);
    }

    public User activate() {
        return withStatus(Status.ACTIVE);
    }

    public User deactivate() {
        return withStatus(Status.INACTIVE);
    }

    public boolean isActive() {
        return status.isActive();
    }

    @Override
    public List<String> validate() {
        List<String> errors = new ArrayList<>();

        if (name == null || name.isBlank()) {
            errors.add("Name cannot be empty");
        }

        if (email == null || !email.contains("@")) {
            errors.add("Invalid email format");
        }

        return errors;
    }

    @Override
    public boolean isValid() {
        return validate().isEmpty();
    }

    public String toJson() {
        return String.format(
                "{\"id\":%s,\"name\":\"%s\",\"email\":\"%s\",\"status\":\"%s\"}",
                id, name, email, status.getValue()
        );
    }
}
