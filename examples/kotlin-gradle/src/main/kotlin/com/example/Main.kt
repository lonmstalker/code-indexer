package com.example

import com.example.impl.InMemoryProductRepository
import com.example.impl.InMemoryUserRepository
import com.example.models.*

/**
 * Singleton configuration object.
 */
object Config {
    const val APP_NAME = "Kotlin Example"
    const val VERSION = "1.0.0"

    fun info() = "$APP_NAME v$VERSION"
}

/**
 * Sealed class for operation results.
 */
sealed class Result<out T> {
    data class Success<T>(val value: T) : Result<T>()
    data class Error(val message: String) : Result<Nothing>()

    fun <R> map(transform: (T) -> R): Result<R> = when (this) {
        is Success -> Success(transform(value))
        is Error -> this
    }

    fun getOrNull(): T? = when (this) {
        is Success -> value
        is Error -> null
    }

    fun getOrThrow(): T = when (this) {
        is Success -> value
        is Error -> throw RuntimeException(message)
    }
}

/**
 * Main function demonstrating Kotlin features.
 */
fun main() {
    println(Config.info())

    demonstrateLambdas()
    demonstrateExtensionFunctions()
    demonstrateScopeFunctions()
    demonstrateWhenExpressions()
}

/**
 * Demonstrate lambda expressions.
 */
fun demonstrateLambdas() {
    val userRepo = InMemoryUserRepository()

    // Create users with builder pattern
    listOf(
        User(name = "Alice", email = "alice@example.com").activate(),
        User(name = "Bob", email = "bob@example.com").activate(),
        User(name = "Charlie", email = "charlie@example.com")
    ).forEach { userRepo.save(it) }

    // Filter with lambda using 'it'
    val activeUsers = userRepo.findAll()
        .filter { it.status == Status.ACTIVE }

    println("Active users: ${activeUsers.size}")

    // Map with lambda
    val userNames = userRepo.findAll()
        .map { it.name }

    println("User names: $userNames")

    // Find with lambda
    val alice = userRepo.findAll()
        .find { it.name == "Alice" }

    alice?.let { println("Found: ${it.toJson()}") }
}

/**
 * Demonstrate extension functions.
 */
fun demonstrateExtensionFunctions() {
    val userRepo = InMemoryUserRepository()

    val user = userRepo.save(User(name = "Diana", email = "diana@example.com"))

    // Use extension function
    val dto = user.toDTO()
    println("User DTO: $dto")

    // Products extension
    val productRepo = InMemoryProductRepository()
    productRepo.save(Product(name = "Laptop", price = 999.99))
    productRepo.save(Product(name = "Mouse", price = 29.99))

    val total = productRepo.findAll().totalPrice()
    println("Total price: $total")
}

/**
 * Demonstrate scope functions.
 */
fun demonstrateScopeFunctions() {
    val userRepo = InMemoryUserRepository()

    // let
    val user = userRepo.save(User(name = "Eve", email = "eve@example.com"))
        .let { it.activate() }
        .also { println("Activated: ${it.name}") }

    // apply
    val productRepo = InMemoryProductRepository().apply {
        save(Product(name = "Keyboard", price = 79.99))
        save(Product(name = "Monitor", price = 299.99))
    }

    println("Products: ${productRepo.count()}")

    // run
    val expensiveCount = productRepo.run {
        findAll().count { it.price > 100 }
    }

    println("Expensive products: $expensiveCount")

    // with
    with(productRepo) {
        val inStock = findInStock()
        println("In stock: ${inStock.size}")
    }
}

/**
 * Demonstrate when expressions.
 */
fun demonstrateWhenExpressions() {
    val status = Status.ACTIVE

    // when as expression
    val message = when (status) {
        Status.ACTIVE -> "User is active"
        Status.INACTIVE -> "User is inactive"
        Status.PENDING -> "User is pending"
    }

    println(message)

    // when with conditions
    val price = 150.0
    val category = when {
        price < 50 -> "cheap"
        price < 200 -> "moderate"
        else -> "expensive"
    }

    println("Price category: $category")

    // when with sealed class
    val result: Result<User> = Result.Success(User(name = "Test", email = "test@example.com"))

    when (result) {
        is Result.Success -> println("Success: ${result.value.name}")
        is Result.Error -> println("Error: ${result.message}")
    }
}

/**
 * Higher-order function example.
 */
inline fun <T> Repository<T>.findAndProcess(
    id: Long,
    onFound: (T) -> Unit,
    onNotFound: () -> Unit
) {
    val entity = findById(id)
    if (entity != null) {
        onFound(entity)
    } else {
        onNotFound()
    }
}

/**
 * Generic function with reified type parameter.
 */
inline fun <reified T> printType(value: T) {
    println("Type: ${T::class.simpleName}, Value: $value")
}
