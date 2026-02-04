import {
  InMemoryProductRepository,
  InMemoryUserRepository,
} from "./implementations";
import { Status, isUser, isProduct, Repository, Entity } from "./interfaces";
import { Product, User, calculateTotalPrice } from "./models";

/**
 * Main function demonstrating TypeScript features.
 */
function main(): void {
  demonstrateArrowFunctions();
  demonstrateGenerics();
  demonstrateTypeGuards();
  demonstrateHigherOrderFunctions();
}

/**
 * Demonstrate arrow functions and lambdas.
 */
function demonstrateArrowFunctions(): void {
  const userRepo = new InMemoryUserRepository();

  // Arrow function for creating users
  const createUser = (name: string, email: string): User =>
    new User(name, email);

  // Save users with arrow functions
  [
    createUser("Alice", "alice@example.com"),
    createUser("Bob", "bob@example.com"),
    createUser("Charlie", "charlie@example.com"),
  ].forEach((user) => userRepo.save(user.activate()));

  // Filter with arrow function
  const activeUsers = userRepo.findAll().filter((u) => u.isActive);

  console.log(`Active users: ${activeUsers.length}`);

  // Map with arrow function
  const userNames = userRepo.findAll().map((u) => u.name);

  console.log(`User names: ${userNames.join(", ")}`);

  // Find with arrow function
  const alice = userRepo.findAll().find((u) => u.name === "Alice");

  if (alice) {
    console.log(`Found: ${alice.toJSON()}`);
  }
}

/**
 * Demonstrate generics.
 */
function demonstrateGenerics(): void {
  // Generic function
  const first = <T>(arr: T[]): T | undefined => arr[0];

  const numbers = [1, 2, 3];
  const strings = ["a", "b", "c"];

  console.log(`First number: ${first(numbers)}`);
  console.log(`First string: ${first(strings)}`);

  // Generic with constraints
  const getIds = <T extends Entity>(entities: T[]): number[] =>
    entities.filter((e) => e.id !== undefined).map((e) => e.id!);

  const userRepo = new InMemoryUserRepository();
  userRepo.save(new User("Test", "test@example.com"));
  userRepo.save(new User("Test2", "test2@example.com"));

  const ids = getIds(userRepo.findAll());
  console.log(`User IDs: ${ids.join(", ")}`);
}

/**
 * Demonstrate type guards.
 */
function demonstrateTypeGuards(): void {
  const items: unknown[] = [
    { name: "Alice", email: "alice@example.com", status: Status.Active },
    { name: "Laptop", price: 999.99, inStock: true },
    { invalid: "data" },
  ];

  items.forEach((item, index) => {
    if (isUser(item)) {
      console.log(`Item ${index} is a User: ${item.name}`);
    } else if (isProduct(item)) {
      console.log(`Item ${index} is a Product: ${item.name}`);
    } else {
      console.log(`Item ${index} is unknown`);
    }
  });
}

/**
 * Demonstrate higher-order functions.
 */
function demonstrateHigherOrderFunctions(): void {
  const productRepo = new InMemoryProductRepository();

  // Save products
  productRepo.save(new Product("Laptop", 999.99));
  productRepo.save(new Product("Mouse", 29.99));
  productRepo.save(new Product("Keyboard", 79.99));
  productRepo.save(new Product("Monitor", 299.99));

  // Higher-order function: processor
  const processProducts = <R>(
    products: Product[],
    processor: (p: Product) => R
  ): R[] => products.map(processor);

  const names = processProducts(productRepo.findAll(), (p) => p.name);
  console.log(`Product names: ${names.join(", ")}`);

  // Higher-order function: filter factory
  const createPriceFilter =
    (maxPrice: number) =>
    (product: Product): boolean =>
      product.price <= maxPrice;

  const affordableFilter = createPriceFilter(100);
  const affordable = productRepo.findAll().filter(affordableFilter);
  console.log(`Affordable products: ${affordable.length}`);

  // Chaining operations
  const total = productRepo
    .findAll()
    .filter((p) => p.inStock)
    .map((p) => p.price)
    .reduce((sum, price) => sum + price, 0);

  console.log(`Total price of in-stock products: ${total}`);

  // Using utility function
  const totalPrice = calculateTotalPrice(productRepo.findAll());
  console.log(`Total price: ${totalPrice}`);
}

/**
 * Generic repository helper functions.
 */
const findAndProcess = <T extends Entity>(
  repo: Repository<T>,
  id: number,
  onFound: (entity: T) => void,
  onNotFound: () => void
): void => {
  const entity = repo.findById(id);
  if (entity) {
    onFound(entity);
  } else {
    onNotFound();
  }
};

/**
 * Curry function example.
 */
const curry =
  <A, B, C>(fn: (a: A, b: B) => C) =>
  (a: A) =>
  (b: B): C =>
    fn(a, b);

/**
 * Compose function example.
 */
const compose =
  <A, B, C>(f: (b: B) => C, g: (a: A) => B) =>
  (a: A): C =>
    f(g(a));

/**
 * Pipe function example.
 */
const pipe =
  <A, B, C>(f: (a: A) => B, g: (b: B) => C) =>
  (a: A): C =>
    g(f(a));

// Run main
main();

export {
  main,
  demonstrateArrowFunctions,
  demonstrateGenerics,
  demonstrateTypeGuards,
  demonstrateHigherOrderFunctions,
  findAndProcess,
  curry,
  compose,
  pipe,
};
