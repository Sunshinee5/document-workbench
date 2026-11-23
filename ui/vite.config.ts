import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Vite 配置：Tauri 桌面壳使用固定端口，避免端口漂移导致 IPC 失效。
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
});
