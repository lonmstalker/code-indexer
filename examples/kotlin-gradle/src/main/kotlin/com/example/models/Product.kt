package com.example.models

import com.example.interfaces.JsonSerializable
import com.example.interfaces.Validator
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.math.BigDecimal
import java.math.RoundingMode

/**
 * Product data class.
 */
@Serializable
data class Product(
    val id: Long? = null,
    val name: String,
    val price: Double,
    val inStock: Boolean = true
) : Validator, JsonSerializable {

    val isAvailable: Boolean get() = inStock && price > 0

    fun withId(id: Long) = copy(id = id)

    fun withStock(inStock: Boolean) = copy(inStock = inStock)

    fun applyDiscount(percent: Double): Product {
        val newPrice = price * (1 - percent / 100)
        return copy(price = BigDecimal(newPrice).setScale(2, RoundingMode.HALF_UP).toDouble())
    }

    override fun validate(): List<String> = buildList {
        if (name.isBlank()) add("Name cannot be empty")
        if (price < 0) add("Price cannot be negative")
    }

    override fun toJson(): String = Json.encodeToString(this)

    override fun toPrettyJson(): String = Json { prettyPrint = true }.encodeToString(this)
}

/**
 * Extension function to calculate total price.
 */
fun List<Product>.totalPrice(): Double = sumOf { it.price }

/**
 * Type alias for product list.
 */
typealias ProductList = List<Product>
