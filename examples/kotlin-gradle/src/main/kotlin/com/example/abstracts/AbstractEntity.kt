package com.example.abstracts

import com.example.interfaces.JsonSerializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Abstract base class for all entities.
 */
abstract class AbstractEntity : JsonSerializable {

    abstract var id: Long?

    val isNew: Boolean
        get() = id == null || id == 0L

    companion object {
        val json = Json { prettyPrint = false }
        val prettyJson = Json { prettyPrint = true }
    }

    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (other == null || javaClass != other.javaClass) return false
        other as AbstractEntity
        return id != null && id == other.id
    }

    override fun hashCode(): Int = id?.hashCode() ?: 0
}
