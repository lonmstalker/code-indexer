import { Entity, Repository, Serializable, Validator } from "./interfaces";

/**
 * Abstract base class for all entities.
 */
export abstract class AbstractEntity implements Entity, Serializable {
  id?: number;

  get isNew(): boolean {
    return this.id === undefined || this.id === 0;
  }

  toJSON(): string {
    return JSON.stringify(this);
  }

  abstract clone(): AbstractEntity;
}

/**
 * Abstract base repository with common functionality.
 */
export abstract class AbstractRepository<T extends Entity>
  implements Repository<T>
{
  abstract findById(id: number): T | undefined;
  abstract findAll(): T[];
  abstract save(entity: T): T;
  abstract delete(id: number): boolean;

  /**
   * Find entities matching a predicate.
   */
  findBy(predicate: (entity: T) => boolean): T[] {
    return this.findAll().filter(predicate);
  }

  /**
   * Find first entity matching a predicate.
   */
  findFirstBy(predicate: (entity: T) => boolean): T | undefined {
    return this.findAll().find(predicate);
  }

  /**
   * Save multiple entities.
   */
  saveAll(entities: T[]): T[] {
    return entities.map((entity) => this.save(entity));
  }

  /**
   * Delete multiple entities by IDs.
   */
  deleteAll(ids: number[]): number {
    return ids.filter((id) => this.delete(id)).length;
  }

  /**
   * Check if entity exists.
   */
  exists(id: number): boolean {
    return this.findById(id) !== undefined;
  }

  /**
   * Count all entities.
   */
  count(): number {
    return this.findAll().length;
  }

  /**
   * Check if any entity matches the predicate.
   */
  existsBy(predicate: (entity: T) => boolean): boolean {
    return this.findAll().some(predicate);
  }

  /**
   * Count entities matching a predicate.
   */
  countBy(predicate: (entity: T) => boolean): number {
    return this.findAll().filter(predicate).length;
  }
}

/**
 * Abstract validatable entity.
 */
export abstract class ValidatableEntity
  extends AbstractEntity
  implements Validator
{
  abstract validate(): string[];

  isValid(): boolean {
    return this.validate().length === 0;
  }
}
