import { discountRate } from './math';

// 服务端模块及其私有依赖只进入服务端产物。
export async function calculateDiscount(amount: number): Promise<number> {
  if (typeof amount !== 'number' || !Number.isFinite(amount) || amount < 0) {
    throw new Error('金额必须是非负有限数字');
  }
  return await Promise.resolve(discountRate(amount));
}
