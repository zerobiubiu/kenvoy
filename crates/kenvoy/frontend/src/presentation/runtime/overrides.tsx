import { Component } from 'react';
import type { ComponentType, ErrorInfo, ReactNode } from 'react';
import { components } from 'virtual:kenvoy-registry';
import { useRuntime } from './context';
import { Notice } from '../ui';
import type { SystemComponentName } from '../../domain/types';
import { useI18n } from '../i18n';

export function OverrideView<T extends { children?: ReactNode }>({ name, model, children }: { name: SystemComponentName; model: T; children: ReactNode }) {
  const { t } = useI18n();
  const { application } = useRuntime();
  const module = application.component_overrides?.[name];
  if (!module) return <>{children}</>;
  const Custom = components[module] as ComponentType<T> | undefined;
  if (!Custom) return <Notice kind="error">{t('ui.component.override_unavailable')}</Notice>;
  return <ComponentBoundary key={module}><Custom {...model}>{children}</Custom></ComponentBoundary>;
}

function ComponentFailure() {
  const { t } = useI18n();
  return <Notice kind="error">{t('ui.component.failed')}</Notice>;
}

export class ComponentBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(_error: Error, _info: ErrorInfo) { /* 用户组件异常只影响当前区域。 */ }
  render() { return this.state.failed ? <ComponentFailure /> : this.props.children; }
}
