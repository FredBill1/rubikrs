import { defineConfig } from '@playwright/test'

const previewPort = Number.parseInt(process.env.RUBIKRS_PREVIEW_PORT ?? '4173', 10)
const baseURL = `http://127.0.0.1:${previewPort}`

export default defineConfig({
  testDir: './tests',
  testMatch: 'fps-benchmark.spec.ts',
  fullyParallel: false,
  workers: 1,
  timeout: 10 * 60 * 1000,
  reporter: [['list']],
  use: {
    baseURL,
    browserName: 'chromium',
    channel: 'msedge',
    headless: true,
    viewport: {
      width: 1920,
      height: 1080,
    },
    launchOptions: {
      args: [
        '--disable-frame-rate-limit',
        '--disable-gpu-vsync',
        '--disable-backgrounding-occluded-windows',
        '--disable-renderer-backgrounding',
        '--disable-background-timer-throttling',
        '--disable-features=CalculateNativeWinOcclusion',
        '--enable-gpu-rasterization',
        '--enable-zero-copy',
        '--enable-webgl',
        '--ignore-gpu-blocklist',
        '--use-angle=d3d11',
      ],
    },
  },
  webServer: {
    command: `npm run build && npm run preview -- --host 127.0.0.1 --port ${previewPort}`,
    cwd: process.cwd(),
    url: baseURL,
    reuseExistingServer: true,
    timeout: 10 * 60 * 1000,
  },
})
