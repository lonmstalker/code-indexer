package com.example.models

import com.example.interfaces.JsonSerializable
import com.example.interfaces.Validator
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * User data class.
 */
@Serializable
data class User(
    val id: Long? = null,
    val name: String,
    val email: String,
    val status: Status = Status.PENDING
) : Validator, JsonSerializable {

    val isActive: Boolean get() = status.isActive

    fun withId(id: Long) = copy(id = id)

    fun withStatus(status: Status) = copy(status = status)

    fun activate() = withStatus(Status.ACTIVE)

    fun deactivate() = withStatus(Status.INACTIVE)

    override fun validate(): List<String> = buildList {
        if (name.isBlank()) add("Name cannot be empty")
        if (!email.contains("@")) add("Invalid email format")
    }

    override fun toJson(): String = Json.encodeToString(this)

    override fun toPrettyJson(): String = Json { prettyPrint = true }.encodeToString(this)
}

/**
 * Extension function to convert User to DTO.
 */
fun User.toDTO(): UserDTO = UserDTO(name, email)

/**
 * User DTO without ID.
 */
@Serializable
data class UserDTO(
    val name: String,
    val email: String
)

/**
 * Type alias for user list.
 */
typealias UserList = List<User>
