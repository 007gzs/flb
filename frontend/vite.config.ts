import path from 'node:path'
import Vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import Components from 'unplugin-vue-components/vite'
import VueRouter from 'unplugin-vue-router/vite'
import { defineConfig } from 'vite'

export default defineConfig({
  resolve: {
    alias: {
      '~/': `${path.resolve(__dirname, 'src')}/`,
    },
  },
  plugins: [
    VueRouter({
      extensions: ['.vue'],
      dts: 'src/typed-router.d.ts',
    }),
    Vue(),
    Components({
      dts: 'src/components.d.ts',
    }),
    UnoCSS(),
  ],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://127.0.0.1:9000',
    },
  },
})
