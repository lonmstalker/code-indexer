package com.example.impl

import com.example.interfaces.Repository
import com.example.models.Product
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/**
 * In-memory implementation of Product repository.
 */
class InMemoryProductRepository : Repository<Product> {

    private val storage = ConcurrentHashMap<Long, Product>()
    private val idGenerator = AtomicLong(1)

    override fun findById(id: Long): Product? = storage[id]

    override fun findAll(): List<Product> = storage.values.toList()

    override fun save(entity: Product): Product {
        val toSave = if (entity.id == null) {
            entity.withId(idGenerator.getAndIncrement())
        } else {
            entity
        }
        storage[toSave.id!!] = toSave
        return toSave
    }

    override fun delete(id: Long): Boolean = storage.remove(id) != null

    fun findInStock(): List<Product> =
        storage.values.filter { it.inStock }

    fun findByPriceRange(min: Double, max: Double): List<Product> =
        storage.values.filter { it.price in min..max }

    fun findByNameContaining(namePart: String): List<Product> =
        storage.values.filter { it.name.contains(namePart, ignoreCase = true) }

    fun calculateTotalValue(): Double =
        storage.values.sumOf { it.price }
}
