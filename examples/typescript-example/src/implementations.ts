import { AbstractRepository } from "./abstracts";
import { IProduct, IUser, Status } from "./interfaces";
import { Product, User } from "./models";

/**
 * In-memory implementation of User repository.
 */
export class InMemoryUserRepository extends AbstractRepository<User> {
  private storage: Map<number, User> = new Map();
  private nextId: number = 1;

  findById(id: number): User | undefined {
    return this.storage.get(id);
  }

  findAll(): User[] {
    return Array.from(this.storage.values());
  }

  save(entity: User): User {
    const toSave =
      entity.id === undefined ? entity.withId(this.nextId++) : entity;
    this.storage.set(toSave.id!, toSave);
    return toSave;
  }

  delete(id: number): boolean {
    return this.storage.delete(id);
  }

  findByEmail(email: string): User | undefined {
    return this.findFirstBy((u) => u.email === email);
  }

  findByStatus(status: Status): User[] {
    return this.findBy((u) => u.status === status);
  }

  findActive(): User[] {
    return this.findByStatus(Status.Active);
  }

  findByNameContaining(namePart: string): User[] {
    return this.findBy((u) =>
      u.name.toLowerCase().includes(namePart.toLowerCase())
    );
  }
}

/**
 * LocalStorage-based implementation of User repository.
 */
export class LocalStorageUserRepository extends AbstractRepository<User> {
  private readonly storageKey: string;

  constructor(storageKey: string = "users") {
    super();
    this.storageKey = storageKey;
  }

  findById(id: number): User | undefined {
    return this.loadAll().find((u) => u.id === id);
  }

  findAll(): User[] {
    return this.loadAll();
  }

  save(entity: User): User {
    const users = this.loadAll();
    const toSave =
      entity.id === undefined ? entity.withId(this.generateId(users)) : entity;

    const index = users.findIndex((u) => u.id === toSave.id);
    if (index >= 0) {
      users[index] = toSave;
    } else {
      users.push(toSave);
    }

    this.saveAll(users);
    return toSave;
  }

  delete(id: number): boolean {
    const users = this.loadAll();
    const filtered = users.filter((u) => u.id !== id);

    if (filtered.length < users.length) {
      this.saveAll(filtered);
      return true;
    }

    return false;
  }

  private loadAll(): User[] {
    try {
      const data = localStorage.getItem(this.storageKey);
      if (!data) return [];

      const parsed = JSON.parse(data) as IUser[];
      return parsed.map((u) => new User(u.name, u.email, u.status, u.id));
    } catch {
      return [];
    }
  }

  private saveAll(users: User[]): void {
    localStorage.setItem(this.storageKey, JSON.stringify(users));
  }

  private generateId(users: User[]): number {
    const maxId = Math.max(0, ...users.map((u) => u.id ?? 0));
    return maxId + 1;
  }
}

/**
 * In-memory implementation of Product repository.
 */
export class InMemoryProductRepository extends AbstractRepository<Product> {
  private storage: Map<number, Product> = new Map();
  private nextId: number = 1;

  findById(id: number): Product | undefined {
    return this.storage.get(id);
  }

  findAll(): Product[] {
    return Array.from(this.storage.values());
  }

  save(entity: Product): Product {
    const toSave =
      entity.id === undefined ? entity.withId(this.nextId++) : entity;
    this.storage.set(toSave.id!, toSave);
    return toSave;
  }

  delete(id: number): boolean {
    return this.storage.delete(id);
  }

  findInStock(): Product[] {
    return this.findBy((p) => p.inStock);
  }

  findByPriceRange(min: number, max: number): Product[] {
    return this.findBy((p) => p.price >= min && p.price <= max);
  }

  findByNameContaining(namePart: string): Product[] {
    return this.findBy((p) =>
      p.name.toLowerCase().includes(namePart.toLowerCase())
    );
  }

  calculateTotalValue(): number {
    return this.findAll().reduce((sum, p) => sum + p.price, 0);
  }
}

/**
 * LocalStorage-based implementation of Product repository.
 */
export class LocalStorageProductRepository extends AbstractRepository<Product> {
  private readonly storageKey: string;

  constructor(storageKey: string = "products") {
    super();
    this.storageKey = storageKey;
  }

  findById(id: number): Product | undefined {
    return this.loadAll().find((p) => p.id === id);
  }

  findAll(): Product[] {
    return this.loadAll();
  }

  save(entity: Product): Product {
    const products = this.loadAll();
    const toSave =
      entity.id === undefined
        ? entity.withId(this.generateId(products))
        : entity;

    const index = products.findIndex((p) => p.id === toSave.id);
    if (index >= 0) {
      products[index] = toSave;
    } else {
      products.push(toSave);
    }

    this.saveAll(products);
    return toSave;
  }

  delete(id: number): boolean {
    const products = this.loadAll();
    const filtered = products.filter((p) => p.id !== id);

    if (filtered.length < products.length) {
      this.saveAll(filtered);
      return true;
    }

    return false;
  }

  private loadAll(): Product[] {
    try {
      const data = localStorage.getItem(this.storageKey);
      if (!data) return [];

      const parsed = JSON.parse(data) as IProduct[];
      return parsed.map((p) => new Product(p.name, p.price, p.inStock, p.id));
    } catch {
      return [];
    }
  }

  private saveAll(products: Product[]): void {
    localStorage.setItem(this.storageKey, JSON.stringify(products));
  }

  private generateId(products: Product[]): number {
    const maxId = Math.max(0, ...products.map((p) => p.id ?? 0));
    return maxId + 1;
  }
}
