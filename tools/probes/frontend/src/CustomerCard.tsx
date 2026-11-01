import { useState } from 'react';

export function CustomerCard({ title }: { title: string }) {
  const [count, setCount] = useState(0);
  return (
    <section className="probe-card">
          <h2>{title}</h2>
          <span data-testid="refresh-label">组件修改前</span>
          <button onClick={() => setCount((value) => value + 1)} data-testid="counter">
            点击次数：{count}
          </button>
    </section>
  );
}
