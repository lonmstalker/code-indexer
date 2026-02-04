package com.example.interfaces;

import java.util.List;

/**
 * Interface for entity validation.
 */
public interface Validator {

    /**
     * Validate the entity.
     *
     * @return list of validation errors, empty if valid
     */
    List<String> validate();

    /**
     * Check if entity is valid.
     *
     * @return true if valid
     */
    default boolean isValid() {
        return validate().isEmpty();
    }
}
