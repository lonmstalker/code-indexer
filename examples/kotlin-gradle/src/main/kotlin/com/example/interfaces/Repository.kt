package com.example.interfaces

/**
 * Generic repository interface for CRUD operations.
 */
interface Repository<T> {

    /**
     * Find entity by ID.
     */
    fun findById(id: Long): T?

    /**
     * Find all entities.
     */
    fun findAll(): List<T>

    /**
     * Save entity.
     */
    fun save(entity: T): T

    /**
     * Delete entity by ID.
     */
    fun delete(id: Long): Boolean

    /**
     * Check if entity exists.
     */
    fun exists(id: Long): Boolean = findById(id) != null

    /**
     * Count all entities.
     */
    fun count(): Long = findAll().size.toLong()
}
