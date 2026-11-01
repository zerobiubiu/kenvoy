import { Slot } from '@radix-ui/react-slot';
import { useId } from 'react';
import { useField, useResource } from '@kenvoy/client';
import type { FormBinding } from '@kenvoy/client';
import './components.css';

/** 只替换显示；值、错误、联动及提交仍属于框架字段绑定。 */
export function CustomSelect() {
  const binding = useField();
  const id = useId();
  return <div className="custom-select" data-field={binding.field.name}>
    <label htmlFor={id}>{binding.field.label}{binding.field.required ? '（必填）' : ''}</label>
    <Slot className="custom-select-control">
      <select id={id} name={binding.field.name} value={binding.draft}
        disabled={binding.disabled || binding.optionsLoading || !binding.optionsReady}
        aria-invalid={Boolean(binding.error)} aria-describedby={`${id}-error`}
        onChange={event => event.target.value ? binding.onChange(event.target.value) : binding.onDraftChange('')}>
        <option value="">请选择</option>
        {binding.options.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
      </select>
    </Slot>
    <p id={`${id}-error`} role={binding.error ? 'alert' : undefined}>{binding.error}</p>
  </div>;
}

export function CustomForm({ resource, children }: FormBinding) {
  return <section className="custom-form" data-resource={resource.name}>{children}</section>;
}

export function ResourceSummary() {
  const { definition } = useResource('entries');
  return <p data-testid="resource-summary">表单使用 {definition.name} 的 {definition.fields.length} 个原生字段。</p>;
}
