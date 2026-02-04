package com.example.models

import kotlinx.serialization.Serializable

/**
 * Status enumeration for entities.
 */
@Serializable
enum class Status(val value: String) {
    ACTIVE("active"),
    INACTIVE("inactive"),
    PENDING("pending");

    val isActive: Boolean get() = this == ACTIVE
    val isInactive: Boolean get() = this == INACTIVE
    val isPending: Boolean get() = this == PENDING

    companion object {
        fun fromValue(value: String): Status =
            entries.find { it.value == value }
                ?: throw IllegalArgumentException("Unknown status: $value")
    }
}
