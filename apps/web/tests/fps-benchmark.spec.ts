import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'

import { expect, test } from '@playwright/test'

type FrameBenchmarkSummary = {
  label: string
  warmupFrames: number
  sampledFrames: number
  durationMs: number
  averageFrameTimeMs: number
  p50FrameTimeMs: number
  p95FrameTimeMs: number
  p99FrameTimeMs: number
  averageFps: number
  p50Fps: number
  minFps: number
  maxFps: number
}

type TurnBenchmarkSummary = FrameBenchmarkSummary & {
  order: number
  seed: number
  maxWidth: number
  turnsApplied: number
  renderer: string
}

type BenchmarkController = {
  waitForReady: () => Promise<void>
  measureAnimationFrameRate: (options?: {
    label?: string
    warmupFrames?: number
    sampleFrames?: number
  }) => Promise<FrameBenchmarkSummary>
  runContinuousTurns: (options?: {
    label?: string
    order?: number
    seed?: number
    warmupFrames?: number
    sampleFrames?: number
    maxWidth?: number
  }) => Promise<TurnBenchmarkSummary>
  runBatchedTurns: (options?: {
    label?: string
    order?: number
    seed?: number
    warmupFrames?: number
    sampleFrames?: number
    batchSize?: number
  }) => Promise<TurnBenchmarkSummary>
}

declare global {
  interface Window {
    __rubikrsBenchmark?: BenchmarkController
  }
}

test.describe('17x17 FPS benchmark', () => {
  test.setTimeout(10 * 60 * 1000)

  test('measures uncapped rAF and continuous 17x17 turn throughput', async ({ page }, testInfo) => {
    await page.goto('/')
    await page.waitForFunction(() => Boolean(window.__rubikrsBenchmark), undefined, {
      timeout: 120_000,
    })
    await page.evaluate(() => window.__rubikrsBenchmark!.waitForReady())

    const raf = await page.evaluate(() =>
      window.__rubikrsBenchmark!.measureAnimationFrameRate({
        label: 'chromium-rAF-cap',
        warmupFrames: 120,
        sampleFrames: 420,
      })
    )

    const minRafFps = Number.parseInt(process.env.RUBIKRS_MIN_RAF_FPS ?? '90', 10)
    expect(raf.averageFps).toBeGreaterThanOrEqual(minRafFps)

    const stress = await page.evaluate(() =>
      window.__rubikrsBenchmark!.runContinuousTurns({
        label: '17x17-continuous-random-turns',
        order: 17,
        seed: 20260520,
        warmupFrames: 240,
        sampleFrames: 1200,
        maxWidth: 2,
      })
    )

    const targetFps = Number.parseInt(process.env.RUBIKRS_TARGET_FPS ?? '0', 10)
    if (targetFps > 0) {
      expect(stress.p50Fps).toBeGreaterThanOrEqual(targetFps)
    }

    const batched = await page.evaluate(() =>
      window.__rubikrsBenchmark!.runBatchedTurns({
        label: '17x17-batched-parallel-turns',
        order: 17,
        seed: 20260520,
        warmupFrames: 240,
        sampleFrames: 1200,
        batchSize: 4,
      })
    )

    const output = {
      generatedAt: new Date().toISOString(),
      browserName: testInfo.project.name,
      viewport: {
        width: 1920,
        height: 1080,
      },
      raf,
      stress,
      batched,
    }

    const outputDir = path.resolve(process.cwd(), 'benchmark-results')
    await mkdir(outputDir, { recursive: true })
    const outputPath = path.join(outputDir, '17x17-fps.json')
    await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8')

    console.log(JSON.stringify(output, null, 2))
  })
})
