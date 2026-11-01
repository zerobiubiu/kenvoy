import { Card } from './ui';
import { discount } from './math';
import type { Amount } from './math';

const attributes = { title: '客户' };
export const amount: Amount = 10000;
export function identity<T>(value: T): T { return value; }
export { discount as calculateDiscount };

export default (
  <>
    <panel title="客户😀" enabled count={3} active={false} {...attributes} onSelect={discount}>
      <Card value={identity(amount)} />
      {amount > 0 ? '有效' : '无效'}
    </panel>
  </>
);
