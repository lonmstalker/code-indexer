package com.example.models;

import com.example.interfaces.Validator;

import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.ArrayList;
import java.util.List;

/**
 * Product entity record.
 */
public record Product(Long id, String name, BigDecimal price, boolean inStock) implements Validator {

    public Product(String name, BigDecimal price) {
        this(null, name, price, true);
    }

    public Product(String name, double price) {
        this(null, name, BigDecimal.valueOf(price), true);
    }

    public Product withId(Long id) {
        return new Product(id, name, price, inStock);
    }

    public Product withStock(boolean inStock) {
        return new Product(id, name, price, inStock);
    }

    public Product applyDiscount(double percent) {
        BigDecimal discount = price.multiply(BigDecimal.valueOf(percent / 100));
        BigDecimal newPrice = price.subtract(discount).setScale(2, RoundingMode.HALF_UP);
        return new Product(id, name, newPrice, inStock);
    }

    public boolean isAvailable() {
        return inStock && price.compareTo(BigDecimal.ZERO) > 0;
    }

    @Override
    public List<String> validate() {
        List<String> errors = new ArrayList<>();

        if (name == null || name.isBlank()) {
            errors.add("Name cannot be empty");
        }

        if (price == null || price.compareTo(BigDecimal.ZERO) < 0) {
            errors.add("Price cannot be negative");
        }

        return errors;
    }

    @Override
    public boolean isValid() {
        return validate().isEmpty();
    }

    public String toJson() {
        return String.format(
                "{\"id\":%s,\"name\":\"%s\",\"price\":%s,\"inStock\":%s}",
                id, name, price, inStock
        );
    }
}
