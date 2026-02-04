/**
 * Generic repository interface for CRUD operations.
 */
export interface Repository<T> {
  /**
   * Find entity by ID.
   */
  findById(id: number): T | undefined;

  /**
   * Find all entities.
   */
  findAll(): T[];

  /**
   * Save entity.
   */
  save(entity: T): T;

  /**
   * Delete entity by ID.
   */
  delete(id: number): boolean;

  /**
   * Check if entity exists.
   */
  exists?(id: number): boolean;

  /**
   * Count all entities.
   */
  count?(): number;
}

/**
 * Interface for JSON serialization support.
 */
export interface Serializable {
  /**
   * Convert object to JSON string.
   */
  toJSON(): string;

  /**
   * Parse from JSON string.
   */
  fromJSON?(json: string): void;
}

/**
 * Interface for entity validation.
 */
export interface Validator {
  /**
   * Validate the entity.
   */
  validate(): string[];

  /**
   * Check if entity is valid.
   */
  isValid(): boolean;
}

/**
 * Base entity interface.
 */
export interface Entity {
  id?: number;
}

/**
 * User entity interface.
 */
export interface IUser extends Entity {
  name: string;
  email: string;
  status: Status;
}

/**
 * Product entity interface.
 */
export interface IProduct extends Entity {
  name: string;
  price: number;
  inStock: boolean;
}

/**
 * Status enumeration.
 */
export enum Status {
  Active = "active",
  Inactive = "inactive",
  Pending = "pending",
}

/**
 * Type alias for user without ID.
 */
export type UserDTO = Omit<IUser, "id">;

/**
 * Type alias for product without ID.
 */
export type ProductDTO = Omit<IProduct, "id">;

/**
 * Type alias for partial user update.
 */
export type UserUpdate = Partial<Omit<IUser, "id">>;

/**
 * Type alias for partial product update.
 */
export type ProductUpdate = Partial<Omit<IProduct, "id">>;

/**
 * Type guard for User.
 */
export function isUser(obj: unknown): obj is IUser {
  return (
    typeof obj === "object" &&
    obj !== null &&
    "name" in obj &&
    "email" in obj &&
    "status" in obj
  );
}

/**
 * Type guard for Product.
 */
export function isProduct(obj: unknown): obj is IProduct {
  return (
    typeof obj === "object" &&
    obj !== null &&
    "name" in obj &&
    "price" in obj &&
    "inStock" in obj
  );
}
