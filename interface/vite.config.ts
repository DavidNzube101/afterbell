import { sveltekit } from '@sveltejs/kit/vite'
import adapter from '@sveltejs/adapter-vercel'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [sveltekit({ adapter: adapter() })],
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        rewrite: path => path.replace(/^\/api/, ''),
      },
    },
  },
})
