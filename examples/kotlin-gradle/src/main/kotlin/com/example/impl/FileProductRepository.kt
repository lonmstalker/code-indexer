package com.example.impl

import com.example.interfaces.Repository
import com.example.models.Product
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File

/**
 * File-based implementation of Product repository.
 */
class FileProductRepository(private val filePath: String) : Repository<Product> {

    private val file = File(filePath)
    private val json = Json { prettyPrint = true }

    override fun findById(id: Long): Product? =
        loadAll().find { it.id == id }

    override fun findAll(): List<Product> = loadAll()

    override fun save(entity: Product): Product {
        val products = loadAll().toMutableList()
        val toSave = if (entity.id == null) {
            entity.withId(generateId(products))
        } else {
            entity
        }

        products.removeAll { it.id == toSave.id }
        products.add(toSave)
        saveAll(products)
        return toSave
    }

    override fun delete(id: Long): Boolean {
        val products = loadAll().toMutableList()
        val removed = products.removeAll { it.id == id }
        if (removed) saveAll(products)
        return removed
    }

    private fun loadAll(): List<Product> {
        if (!file.exists()) return emptyList()
        return try {
            json.decodeFromString<List<Product>>(file.readText())
        } catch (e: Exception) {
            emptyList()
        }
    }

    private fun saveAll(products: List<Product>) {
        file.writeText(json.encodeToString(products))
    }

    private fun generateId(products: List<Product>): Long =
        (products.mapNotNull { it.id }.maxOrNull() ?: 0L) + 1
}
