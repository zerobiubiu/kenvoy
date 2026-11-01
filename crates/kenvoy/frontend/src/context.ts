import { createContext, useContext } from 'react';
import type { ApiClient } from './api';
import type { ApplicationDescription, SessionView } from './types';

export interface RuntimeState {
  application: ApplicationDescription;
  session: SessionView;
  client: ApiClient;
  refresh: () => Promise<void>;
}

// 与组件注册表分离，避免 SDK 导入形成环并在 Fast Refresh 中重建 Context。
export const RuntimeContext = createContext<RuntimeState | null>(null);

export function useRuntime(): RuntimeState {
  const state = useContext(RuntimeContext);
  if (!state) throw new Error('页面上下文尚未准备完成');
  return state;
}
