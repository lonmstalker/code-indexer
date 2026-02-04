import { ValidatableEntity } from "./abstracts";
import { IProduct, IUser, Status, UserDTO, ProductDTO } from "./interfaces";

/**
 * User model class.
 */
export class User extends ValidatableEntity implements IUser {
  constructor(
    public name: string,
    public email: string,
    public status: Status = Status.Pending,
    id?: number
  ) {
    super();
    this.id = id;
  }

  get isActive(): boolean {
    return this.status === Status.Active;
  }

  withId(id: number): User {
    return new User(this.name, this.email, this.status, id);
  }

  withStatus(status: Status): User {
    return new User(this.name, this.email, status, this.id);
  }

  activate(): User {
    return this.withStatus(Status.Active);
  }

  deactivate(): User {
    return this.withStatus(Status.Inactive);
  }

  validate(): string[] {
    const errors: string[] = [];

    if (!this.name || this.name.trim() === "") {
      errors.push("Name cannot be empty");
    }

    if (!this.email || !this.email.includes("@")) {
      errors.push("Invalid email format");
    }

    return errors;
  }

  clone(): User {
    return new User(this.name, this.email, this.status, this.id);
  }

  toDTO(): UserDTO {
    return {
      name: this.name,
      email: this.email,
      status: this.status,
    };
  }

  static fromDTO(dto: UserDTO, id?: number): User {
    return new User(dto.name, dto.email, dto.status, id);
  }
}

/**
 * Product model class.
 */
export class Product extends ValidatableEntity implements IProduct {
  constructor(
    public name: string,
    public price: number,
    public inStock: boolean = true,
    id?: number
  ) {
    super();
    this.id = id;
  }

  get isAvailable(): boolean {
    return this.inStock && this.price > 0;
  }

  withId(id: number): Product {
    return new Product(this.name, this.price, this.inStock, id);
  }

  withStock(inStock: boolean): Product {
    return new Product(this.name, this.price, inStock, this.id);
  }

  applyDiscount(percent: number): Product {
    const newPrice = this.price * (1 - percent / 100);
    return new Product(
      this.name,
      Math.round(newPrice * 100) / 100,
      this.inStock,
      this.id
    );
  }

  validate(): string[] {
    const errors: string[] = [];

    if (!this.name || this.name.trim() === "") {
      errors.push("Name cannot be empty");
    }

    if (this.price < 0) {
      errors.push("Price cannot be negative");
    }

    return errors;
  }

  clone(): Product {
    return new Product(this.name, this.price, this.inStock, this.id);
  }

  toDTO(): ProductDTO {
    return {
      name: this.name,
      price: this.price,
      inStock: this.inStock,
    };
  }

  static fromDTO(dto: ProductDTO, id?: number): Product {
    return new Product(dto.name, dto.price, dto.inStock, id);
  }
}

/**
 * Calculate total price of products.
 */
export const calculateTotalPrice = (products: Product[]): number => {
  return products.reduce((total, product) => total + product.price, 0);
};

/**
 * Filter products by price range.
 */
export const filterByPriceRange = (
  products: Product[],
  min: number,
  max: number
): Product[] => {
  return products.filter((p) => p.price >= min && p.price <= max);
};
