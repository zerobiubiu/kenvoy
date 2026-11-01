import { discount, identity } from './math.js';

interface Input {
  name: string;
  amount: number;
}

export function calculate(input: Input): { name: string; total: number } {
  return identity({ name: input.name, total: input.amount * discount(input.amount) });
}

export async function calculateAsync(input: Input): Promise<number> {
  const amount: number = await Promise.resolve(input.amount);
  return amount * discount(amount);
}
