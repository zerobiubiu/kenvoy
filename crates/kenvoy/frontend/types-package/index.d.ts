/** 由 compilerOptions.types 加载；运行值仍由 Kenvoy CLI 的虚拟模块提供。 */
declare module '@kenvoy/client' {
  export * from '@kenvoy/types/sdk';
}

declare module '@kenvoy/client/jsx-runtime' {
  export * from '@kenvoy/types/jsx-runtime';
}

declare module '@kenvoy/client/jsx-dev-runtime' {
  export * from '@kenvoy/types/jsx-dev-runtime';
}
