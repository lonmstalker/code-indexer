package com.example.interfaces

/**
 * Interface for JSON serialization support.
 */
interface JsonSerializable {

    /**
     * Convert object to JSON string.
     */
    fun toJson(): String

    /**
     * Pretty print JSON.
     */
    fun toPrettyJson(): String = toJson()
}
