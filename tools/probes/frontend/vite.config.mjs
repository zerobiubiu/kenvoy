import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// 仅为兼容性探针；正式框架配置仍需 Kenvoy 适配层管理。
export default defineConfig({
  plugins: [
    {
      name: 'probe-server-boundary',
      enforce: 'pre',
      load(id) {
        if (/\.server\.[cm]?[jt]sx?$/.test(id.split('?')[0])) {
          throw new Error('P04_SERVER_BOUNDARY: 客户端图不得引入 server 模块');
        }
      },
    },
    react(),
  ],
  server: { host: '127.0.0.1', port: 0, strictPort: false },
  build: { sourcemap: true, manifest: true },
});
