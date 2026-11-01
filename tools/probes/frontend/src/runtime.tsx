import { useState, type ChangeEvent } from 'react';
import { CustomerCard } from './CustomerCard';
import description from './description.json';

type Field = typeof description.page.fields[number];
type FieldProps = { field: Field; value: string; onChange: (event: ChangeEvent<HTMLInputElement>) => void };

const registry = {
  text: ({ field, value, onChange }: FieldProps) => (
    <label>{field.label}<input name={field.name} required={Boolean(field.required)}
      value={value} onChange={onChange} maxLength={field.maxLength} /></label>
  ),
  number: ({ field, value, onChange }: FieldProps) => (
    <label>{field.label}<input name={field.name} type="number" value={value}
      onChange={onChange} min={field.min} step={field.integer ? 1 : 'any'} /></label>
  ),
};

// 只证明 JSON 描述可以驱动 React，不执行 Generic IR 或数据库逻辑。
export function PageInterpreter() {
  const [values, setValues] = useState<Record<string, string>>({});
  const [submitted, setSubmitted] = useState(false);
  return (
    <main>
      <h1>{description.page.title}</h1>
      <p role="note">独立技术探针；表单仅验证浏览器交互，不连接数据库。</p>
      <form onSubmit={(event) => { event.preventDefault(); setSubmitted(true); }}>
        {description.page.fields.map((field) => {
          const Component = registry[field.kind as keyof typeof registry];
          return <Component key={field.name} field={field} value={values[field.name] ?? ''}
            onChange={(event) => setValues((current) => ({ ...current, [field.name]: event.target.value }))} />;
        })}
        <button type="submit">验证交互</button>
      </form>
      {submitted && <p role="status">浏览器表单交互完成；未写入数据库。</p>}
      <CustomerCard title="跨文件组件与 Props" />
      <span className="css-probe" data-testid="css-probe">CSS 更新探针</span>
    </main>
  );
}
