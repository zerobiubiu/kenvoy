declare module 'virtual:kenvoy-registry' {
  import type { ComponentType } from 'react';
  export const components: Record<string, ComponentType<Record<string, unknown>>>;
  export const functions: Record<string, (...args: unknown[]) => unknown>;
  export const expressions: Record<string, () => unknown>;
}
