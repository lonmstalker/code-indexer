package com.example.interfaces;

import java.util.List;
import java.util.Optional;

/**
 * Generic repository interface for CRUD operations.
 *
 * @param <T> the entity type
 */
public interface Repository<T> {

    /**
     * Find entity by ID.
     *
     * @param id the entity ID
     * @return optional containing the entity if found
     */
    Optional<T> findById(Long id);

    /**
     * Find all entities.
     *
     * @return list of all entities
     */
    List<T> findAll();

    /**
     * Save entity.
     *
     * @param entity the entity to save
     * @return the saved entity with ID
     */
    T save(T entity);

    /**
     * Delete entity by ID.
     *
     * @param id the entity ID
     * @return true if deleted
     */
    boolean delete(Long id);

    /**
     * Check if entity exists.
     *
     * @param id the entity ID
     * @return true if exists
     */
    default boolean exists(Long id) {
        return findById(id).isPresent();
    }

    /**
     * Count all entities.
     *
     * @return the count
     */
    default long count() {
        return findAll().size();
    }
}
