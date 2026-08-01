import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [
    vue(),
    tailwindcss(),
  ],

  server: {
    port: 14300,
    strictPort: true,
    // 💡 关键：禁止 Vite 监听 Rust 的 target 编译目录，以及后端写入的 bookmarks.json
    // （save_bookmarks 每次保存都会往项目根目录写这个文件，不排除的话会被 Vite 当成源码变化触发整页刷新）
    watch: {
      ignored: ['**/src-tauri/**', '**/bookmarks.json'],
    },
  },
}))