import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  resolve: { alias: { '@kenvoy/client': fileURLToPath(new URL('./src/sdk.ts', import.meta.url)) } },
  plugins: [
    {
      name: 'kenvoy-default-registry',
      resolveId(id) { if (id === 'virtual:kenvoy-registry') return '\0kenvoy-registry'; },
      load(id) {
        if (id === '\0kenvoy-registry') return 'export const components = {}; export const functions = {}; export const expressions = {};';
        if (/\.server\.[cm]?[jt]sx?(?:\?|$)/.test(id)) throw new Error('客户端不得导入服务端私有模块');
      },
    },
    react(),
  ],
  build: { outDir: 'dist', manifest: true, sourcemap: false },
});
