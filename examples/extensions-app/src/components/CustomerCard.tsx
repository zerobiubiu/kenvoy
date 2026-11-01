import { useState } from 'react';
import { Slot } from '@radix-ui/react-slot';
import clsx from 'clsx';
import { ApiError, useAction } from '@kenvoy/client';

interface CustomerCardProps {
  title: string;
  onIncrement: (count: number) => number;
}

export function CustomerCard({ title, onIncrement }: CustomerCardProps) {
  const [count, setCount] = useState(0);
  const [discount, setDiscount] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState('');
  const calculate = useAction('calculateDiscount');

  async function calculateOnce() {
    if (busy) return;
    setBusy(true);
    setFailure('');
    try {
      const result = await calculate([10000]);
      if (typeof result !== 'number') throw new Error('服务端返回值无效');
      setDiscount(result);
    } catch (error) {
      setFailure(error instanceof ApiError && error.outcomeUnknown
        ? '请求结果待核对，系统没有自动重试。'
        : error instanceof Error ? error.message : '计算未完成');
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={clsx('customer-card-example', { active: count > 0 })}>
          <h2>{title}</h2>
          <p>数量：{count}</p>
          <div className="customer-card-actions">
            <Slot onClick={() => setCount(onIncrement(count))}><button type="button">增加数量</button></Slot>
            <button type="button" disabled={busy} onClick={() => { void calculateOnce(); }}>
              {busy ? '正在计算…' : '计算折扣'}
            </button>
          </div>
          {discount !== null && <p>折扣：{discount}</p>}
          {failure && <p role="alert">{failure}</p>}
    </section>
  );
}
