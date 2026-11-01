export function discount(amount: number): number {
  return amount >= 10000 ? 0.8 : 0.9;
}

export function identity<T>(value: T): T {
  return value;
}
