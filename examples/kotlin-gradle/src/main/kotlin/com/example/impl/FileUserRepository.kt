package com.example.impl

import com.example.interfaces.Repository
import com.example.models.User
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File

/**
 * File-based implementation of User repository.
 */
class FileUserRepository(private val filePath: String) : Repository<User> {

    private val file = File(filePath)
    private val json = Json { prettyPrint = true }

    override fun findById(id: Long): User? =
        loadAll().find { it.id == id }

    override fun findAll(): List<User> = loadAll()

    override fun save(entity: User): User {
        val users = loadAll().toMutableList()
        val toSave = if (entity.id == null) {
            entity.withId(generateId(users))
        } else {
            entity
        }

        users.removeAll { it.id == toSave.id }
        users.add(toSave)
        saveAll(users)
        return toSave
    }

    override fun delete(id: Long): Boolean {
        val users = loadAll().toMutableList()
        val removed = users.removeAll { it.id == id }
        if (removed) saveAll(users)
        return removed
    }

    private fun loadAll(): List<User> {
        if (!file.exists()) return emptyList()
        return try {
            json.decodeFromString<List<User>>(file.readText())
        } catch (e: Exception) {
            emptyList()
        }
    }

    private fun saveAll(users: List<User>) {
        file.writeText(json.encodeToString(users))
    }

    private fun generateId(users: List<User>): Long =
        (users.mapNotNull { it.id }.maxOrNull() ?: 0L) + 1
}
