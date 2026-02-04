package com.example.abstracts;

import com.example.interfaces.Repository;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Predicate;
import java.util.stream.Collectors;

/**
 * Abstract base repository with common functionality.
 *
 * @param <T> the entity type extending AbstractEntity
 */
public abstract class AbstractRepository<T extends AbstractEntity> implements Repository<T> {

    /**
     * Find entities matching a predicate.
     *
     * @param predicate the filter predicate
     * @return list of matching entities
     */
    public List<T> findBy(Predicate<T> predicate) {
        return findAll().stream()
                .filter(predicate)
                .collect(Collectors.toList());
    }

    /**
     * Find first entity matching a predicate.
     *
     * @param predicate the filter predicate
     * @return the first matching entity or null
     */
    public T findFirstBy(Predicate<T> predicate) {
        return findAll().stream()
                .filter(predicate)
                .findFirst()
                .orElse(null);
    }

    /**
     * Save multiple entities.
     *
     * @param entities the entities to save
     * @return list of saved entities
     */
    public List<T> saveAll(Iterable<T> entities) {
        List<T> result = new ArrayList<>();
        entities.forEach(entity -> result.add(save(entity)));
        return result;
    }

    /**
     * Delete multiple entities by IDs.
     *
     * @param ids the entity IDs
     * @return count of deleted entities
     */
    public long deleteAll(Iterable<Long> ids) {
        long count = 0;
        for (Long id : ids) {
            if (delete(id)) {
                count++;
            }
        }
        return count;
    }

    /**
     * Check if any entity matches the predicate.
     *
     * @param predicate the filter predicate
     * @return true if any match
     */
    public boolean existsBy(Predicate<T> predicate) {
        return findAll().stream().anyMatch(predicate);
    }

    /**
     * Count entities matching a predicate.
     *
     * @param predicate the filter predicate
     * @return the count
     */
    public long countBy(Predicate<T> predicate) {
        return findAll().stream().filter(predicate).count();
    }
}
