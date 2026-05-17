import { spawnSync } from 'node:child_process'
import { resolve } from 'node:path'

const isDev = process.argv.includes('--dev')
const crateDir = resolve(process.cwd(), '..', '..', 'crates', 'rubik-app')
const outDir = resolve(process.cwd(), 'src', 'generated', 'rubik_app')

const args = [
  'build',
  crateDir,
  '--target',
  'web',
  '--out-dir',
  outDir,
  '--out-name',
  'rubik_app',
]

if (isDev) {
  args.push('--dev')
} else {
  args.push('--release')
}

const result = spawnSync('wasm-pack', args, {
  stdio: 'inherit',
  shell: process.platform === 'win32',
})

if (result.error) {
  throw result.error
}

process.exit(result.status ?? 1)
