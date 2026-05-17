import { defineConfig } from 'vite'

export default defineConfig({
  base: process.env.RUBIK_BASE_PATH ?? '/',
  build: {
    sourcemap: true,
  },
})
