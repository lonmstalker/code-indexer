package com.example.interfaces

/**
 * Interface for entity validation.
 */
interface Validator {

    /**
     * Validate the entity.
     */
    fun validate(): List<String>

    /**
     * Check if entity is valid.
     */
    fun isValid(): Boolean = validate().isEmpty()
}
