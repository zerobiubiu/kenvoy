import { useState, type ChangeEvent } from 'react';
import { Alert, Button, Stack, TextField, Typography } from '@mui/material';
import { CustomerCard } from './CustomerCard';
import description from './description.json';

type Field = typeof description.page.fields[number];
type FieldProps = { field: Field; value: string; onChange: (event: ChangeEvent<HTMLInputElement>) => void };

const registry = {
  text: ({ field, value, onChange }: FieldProps) => (
    <TextField name={field.name} label={field.label} required={Boolean(field.required)}
      value={value} onChange={onChange} slotProps={{ htmlInput: { maxLength: field.maxLength } }} />
  ),
  number: ({ field, value, onChange }: FieldProps) => (
    <TextField name={field.name} label={field.label} type="number" value={value}
      onChange={onChange} slotProps={{ htmlInput: { min: field.min, step: field.integer ? 1 : 'any' } }} />
  ),
};

// 只证明 JSON 描述可以驱动 React/MUI，不执行 Generic IR 或数据库逻辑。
export function PageInterpreter() {
  const [values, setValues] = useState<Record<string, string>>({});
  const [submitted, setSubmitted] = useState(false);
  return (
    <Stack component="main" spacing={3} sx={{ maxWidth: 600, mx: 'auto', p: 3 }}>
      <Typography component="h1" variant="h4">{description.page.title}</Typography>
      <Alert severity="info">独立技术探针；表单仅验证浏览器交互，不连接数据库。</Alert>
      <Stack component="form" spacing={2} onSubmit={(event) => { event.preventDefault(); setSubmitted(true); }}>
        {description.page.fields.map((field) => {
          const Component = registry[field.kind as keyof typeof registry];
          return <Component key={field.name} field={field} value={values[field.name] ?? ''}
            onChange={(event) => setValues((current) => ({ ...current, [field.name]: event.target.value }))} />;
        })}
        <Button type="submit" variant="contained">验证交互</Button>
      </Stack>
      {submitted && <Alert severity="success">浏览器表单交互完成；未写入数据库。</Alert>}
      <CustomerCard title="跨文件组件与 Props" />
      <span className="css-probe" data-testid="css-probe">CSS 更新探针</span>
    </Stack>
  );
}
