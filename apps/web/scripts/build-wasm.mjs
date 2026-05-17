import { spawnSync } from 'node:child_process'
import { resolve } from 'node:path'

const isDev = process.argv.includes('--dev')
const buildMode = isDev ? '--dev' : '--release'

const crates = [
  {
    crateDir: resolve(process.cwd(), '..', '..', 'crates', 'rubik-app'),
    outDir: resolve(process.cwd(), 'src', 'generated', 'rubik_app'),
    outName: 'rubik_app',
  },
  {
    crateDir: resolve(process.cwd(), '..', '..', 'crates', 'solver-worker'),
    outDir: resolve(process.cwd(), 'src', 'generated', 'solver_worker'),
    outName: 'solver_worker',
  },
]

let exitCode = 0

for (const { crateDir, outDir, outName } of crates) {
  const result = spawnSync(
    'wasm-pack',
    ['build', crateDir, '--target', 'web', '--out-dir', outDir, '--out-name', outName, buildMode],
    {
      stdio: 'inherit',
      shell: process.platform === 'win32',
    }
  )

  if (result.error) {
    throw result.error
  }

  if ((result.status ?? 1) !== 0) {
    exitCode = result.status ?? 1
    break
  }
}

process.exit(exitCode)
