export function discountRate(amount: number): number {
  return amount >= 10000 ? 0.8 : 0.9;
}
