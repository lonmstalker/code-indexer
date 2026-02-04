package com.example.interfaces;

/**
 * Interface for JSON serialization support.
 */
public interface JsonSerializable {

    /**
     * Convert object to JSON string.
     *
     * @return JSON representation
     */
    String toJson();

    /**
     * Pretty print JSON.
     *
     * @return formatted JSON string
     */
    default String toPrettyJson() {
        return toJson();
    }
}
