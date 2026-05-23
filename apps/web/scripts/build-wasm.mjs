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
    crateDir: resolve(process.cwd(), '..', '..', 'crates', 'rubik-solver'),
    outDir: resolve(process.cwd(), 'src', 'generated', 'rubik_solver'),
    outName: 'rubik_solver',
  },
]

let exitCode = 0

for (const { crateDir, outDir, outName } of crates) {
  const rustFlags = [process.env.RUSTFLAGS, '--cfg getrandom_backend="wasm_js"']
    .filter(Boolean)
    .join(' ')
  const result = spawnSync(
    'wasm-pack',
    ['build', crateDir, '--target', 'web', '--out-dir', outDir, '--out-name', outName, buildMode],
    {
      env: {
        ...process.env,
        RUSTFLAGS: rustFlags,
      },
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
