package com.example.abstracts

import com.example.interfaces.Repository

/**
 * Abstract base repository with common functionality.
 */
abstract class AbstractRepository<T : AbstractEntity> : Repository<T> {

    /**
     * Find entities matching a predicate.
     */
    fun findBy(predicate: (T) -> Boolean): List<T> =
        findAll().filter(predicate)

    /**
     * Find first entity matching a predicate.
     */
    fun findFirstBy(predicate: (T) -> Boolean): T? =
        findAll().find(predicate)

    /**
     * Save multiple entities.
     */
    fun saveAll(entities: Iterable<T>): List<T> =
        entities.map { save(it) }

    /**
     * Delete multiple entities by IDs.
     */
    fun deleteAll(ids: Iterable<Long>): Long =
        ids.count { delete(it) }.toLong()

    /**
     * Check if any entity matches the predicate.
     */
    fun existsBy(predicate: (T) -> Boolean): Boolean =
        findAll().any(predicate)

    /**
     * Count entities matching a predicate.
     */
    fun countBy(predicate: (T) -> Boolean): Long =
        findAll().count(predicate).toLong()
}
