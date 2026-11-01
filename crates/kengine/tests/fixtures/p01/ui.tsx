export function Card<T>({ value }: { value: T }) {
  return <div className="card">{String(value)}</div>;
}
