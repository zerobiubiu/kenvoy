import { Component } from 'react';
import type { ComponentType, ErrorInfo, ReactNode } from 'react';
import { components } from 'virtual:kenvoy-registry';
import { useRuntime } from './context';
import { Notice } from '../ui';
import type { SystemComponentName } from '../../domain/types';

export function OverrideView<T extends { children?: ReactNode }>({ name, model, children }: { name: SystemComponentName; model: T; children: ReactNode }) {
  const { application } = useRuntime();
  const module = application.component_overrides?.[name];
  if (!module) return <>{children}</>;
  const Custom = components[module] as ComponentType<T> | undefined;
  if (!Custom) return <Notice kind="error">系统组件重载尚未准备完成，请刷新页面或联系管理员。</Notice>;
  return <ComponentBoundary key={module}><Custom {...model}>{children}</Custom></ComponentBoundary>;
}

export class ComponentBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(_error: Error, _info: ErrorInfo) { /* 用户组件异常只影响当前区域。 */ }
  render() { return this.state.failed ? <Notice kind="error">此页面组件暂时无法显示，请联系应用管理员。</Notice> : this.props.children; }
}
