package com.example.impl

import com.example.interfaces.Repository
import com.example.models.Status
import com.example.models.User
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong

/**
 * In-memory implementation of User repository.
 */
class InMemoryUserRepository : Repository<User> {

    private val storage = ConcurrentHashMap<Long, User>()
    private val idGenerator = AtomicLong(1)

    override fun findById(id: Long): User? = storage[id]

    override fun findAll(): List<User> = storage.values.toList()

    override fun save(entity: User): User {
        val toSave = if (entity.id == null) {
            entity.withId(idGenerator.getAndIncrement())
        } else {
            entity
        }
        storage[toSave.id!!] = toSave
        return toSave
    }

    override fun delete(id: Long): Boolean = storage.remove(id) != null

    fun findByEmail(email: String): User? =
        storage.values.find { it.email == email }

    fun findByStatus(status: Status): List<User> =
        storage.values.filter { it.status == status }

    fun findActive(): List<User> = findByStatus(Status.ACTIVE)

    fun findByNameContaining(namePart: String): List<User> =
        storage.values.filter { it.name.contains(namePart, ignoreCase = true) }
}
