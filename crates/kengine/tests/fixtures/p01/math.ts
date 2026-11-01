export type Amount = number;
export function discount(amount: Amount): number {
  return amount >= 10000 ? 0.8 : 0.9;
}
