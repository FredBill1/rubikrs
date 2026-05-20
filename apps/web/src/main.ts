import './style.css'

type BootTone = 'booting' | 'ready' | 'fault'

type RuntimeStatus = {
  order: number
  move_count: number
  redo_depth: number
  is_solved: boolean
  timing_active: boolean
  animation_active: boolean
  elapsed_millis: number
  scene_revision: number
  last_message: string
  recent_turns: string[]
}

type SolveTurn = {
  faceCode: number
  rotationCode: number
  startLayer: number
  width: number
  notation: string
}

type SolveWorkerRequest = {
  kind: 'solve'
  requestId: number
  laneId: number
  order: number
  stateJson: string
  targetDepth: number
  allowedFaces: number[]
  turnHistoryJson?: string
}

type SolveWorkerResponse = {
  kind: 'solved' | 'unsolved' | 'error'
  requestId: number
  laneId: number
  turns: SolveTurn[]
  explored: number
  depthLimit: number
  message: string
}

type RubikWasmModule = {
  default: () => Promise<unknown>
  start_app: (canvasId: string, basePath: string) => void
  runtime_status_json: () => string
  export_cube_state: () => string
  export_turn_history_json: () => string
  import_cube_state: (json: string) => boolean
  set_cube_order: (order: number) => boolean
  reset_cube: () => boolean
  undo_turn: () => boolean
  redo_turn: () => boolean
  scramble_cube: (length: number, seed: number | bigint) => boolean
  apply_turn: (faceCode: number, rotationCode: number, startLayer: number, width: number) => boolean
}

const app = document.querySelector<HTMLDivElement>('#app')

if (!app) {
  throw new Error('rubikrs shell could not find the #app mount point')
}

const basePath = import.meta.env.BASE_URL
const parallelLanes = Math.max(1, Math.min(8, Math.floor((navigator.hardwareConcurrency ?? 4) / 2)))
const rootFaceCodes = [0, 1, 2, 3, 4, 5] as const
let runtime: RubikWasmModule | null = null
let statusPollHandle: number | null = null
let solverWorkers: Worker[] = []
let activeSolveRequestId: number | null = null
let activeSolveSceneRevision: number | null = null
let nextSolveRequestId = 0

app.innerHTML = `
  <div class="shell">
    <main class="layout">
      <section class="stage-card" aria-label="Rubik preview stage">
        <div class="stage-grid" aria-hidden="true"></div>
        <canvas id="rubik-canvas" class="stage-canvas" aria-label="Rubik runtime canvas"></canvas>
        <button type="button" class="stage-toggle stage-toggle--landscape" aria-label="Toggle telemetry panel" title="Toggle telemetry panel">
          <span class="toggle-arrow"/>
        </button>
        <div class="stage-caption">
          <p class="stage-label">drag / orbit / zoom</p>
          <button type="button" class="stage-info" aria-label="Show stage controls help">i</button>
          <p class="stage-hint">
            Drag a sticker to turn the cube, drag empty space to orbit, right drag or two-finger drag always
            orbit, and pinch zooms. Keyboard: U R F D L B, 2..9 for layer selection, Alt for wide turns, Shift
            for inverse, Ctrl for 180,
            Backspace undo, Enter redo.
          </p>
          <button type="button" class="stage-toggle stage-toggle--portrait" aria-label="Toggle telemetry panel" title="Toggle telemetry panel">
            <span class="toggle-arrow"/>
          </button>
        </div>
      </section>

      <aside class="telemetry">
        <div class="telemetry-panels">
        <section class="panel">
          <p class="panel-kicker">cube controls</p>
          <div class="control-cluster">
            <label class="field">
              <span>cube order</span>
              <select data-order-select>
                <option value="2">2x2</option>
                <option value="3" selected>3x3</option>
                <option value="4">4x4</option>
                <option value="5">5x5</option>
                <option value="7">7x7</option>
                <option value="17">17x17</option>
              </select>
            </label>

            <div class="action-row">
              <button type="button" data-action="reset">reset</button>
              <button type="button" data-action="undo">undo</button>
              <button type="button" data-action="redo">redo</button>
            </div>

            <div class="action-row action-row--stacked">
              <label class="field">
                <span>scramble length</span>
                <input data-scramble-length type="number" min="1" max="64" value="20" />
              </label>
              <label class="field">
                <span>seed</span>
                <input data-scramble-seed type="number" min="1" max="9999999" value="20260517" />
              </label>
              <button type="button" data-action="scramble">scramble</button>
            </div>
          </div>
        </section>

        <section class="panel">
          <p class="panel-kicker">manual turns</p>
          <div class="control-cluster">
            <div class="action-row action-row--stacked">
              <label class="field">
                <span>turn layer</span>
                <input data-turn-layer type="number" min="1" max="17" value="1" />
              </label>
              <label class="field field--checkbox">
                <span>wide x2</span>
                <input data-turn-wide type="checkbox" />
              </label>
            </div>

            <div class="turn-grid">
              <button type="button" data-turn="0:0">U</button>
              <button type="button" data-turn="1:0">R</button>
              <button type="button" data-turn="2:0">F</button>
              <button type="button" data-turn="3:0">D</button>
              <button type="button" data-turn="4:0">L</button>
              <button type="button" data-turn="5:0">B</button>
              <button type="button" data-turn="0:2">U'</button>
              <button type="button" data-turn="1:2">R'</button>
              <button type="button" data-turn="2:2">F'</button>
              <button type="button" data-turn="3:2">D'</button>
              <button type="button" data-turn="4:2">L'</button>
              <button type="button" data-turn="5:2">B'</button>
            </div>
          </div>
        </section>

        <section class="panel">
          <p class="panel-kicker">solver</p>
          <div class="control-cluster">
            <div class="action-row action-row--stacked">
              <label class="field field--inline">
                <span>search depth</span>
                <input data-solve-depth type="number" min="1" max="8" value="5" />
              </label>
              <div class="action-row action-row--pair">
                <button type="button" data-action="solve">solve</button>
                <button type="button" data-action="cancel-solve">cancel</button>
              </div>
            </div>
            <p class="history-line" data-solver-status>idle</p>
            <p class="body-copy" data-solver-detail>No solve request in flight.</p>
          </div>
        </section>

        <section class="panel">
          <p class="panel-kicker">import / export</p>
          <div class="control-cluster">
            <div class="action-row">
              <button type="button" data-action="export">export json</button>
              <button type="button" data-action="import-file">load file</button>
              <input data-import-file type="file" accept=".json,application/json" hidden />
            </div>
            <label class="field">
              <span>state payload</span>
              <textarea
                data-import-area
                spellcheck="false"
                placeholder='{"version":1,"order":3,"stickers":[...]}'
              ></textarea>
            </label>
            <button type="button" data-action="import">import state</button>
          </div>
        </section>

        </div>
      </aside>
    </main>

    <footer class="status-bar" aria-label="Runtime status">
      <div class="status-bar-track">
        <span class="eyebrow">rubikrs</span>
        <span class="status-pill" data-tone="booting" data-boot-pill>booting</span>
        <span class="status-segment">
          <span class="status-key">order</span>
          <strong data-status-order>3x3</strong>
        </span>
        <span class="status-segment">
          <span class="status-key">moves</span>
          <strong data-status-moves>0</strong>
        </span>
        <span class="status-segment">
          <span class="status-key">timer</span>
          <strong data-status-timer>00:00.0</strong>
        </span>
        <span class="status-segment status-segment--detail">
          <span class="status-key">runtime</span>
          <span class="status-copy" data-boot-copy>Preparing the wasm runtime and canvas bridge.</span>
        </span>
        <span class="status-segment status-segment--history">
          <span class="status-key">recent</span>
          <span class="history-line" data-status-history>—</span>
        </span>
      </div>
    </footer>
  </div>
`

const bootPill = document.querySelector<HTMLElement>('[data-boot-pill]')
const bootCopy = document.querySelector<HTMLElement>('[data-boot-copy]')
const orderSelect = document.querySelector<HTMLSelectElement>('[data-order-select]')
const scrambleLength = document.querySelector<HTMLInputElement>('[data-scramble-length]')
const scrambleSeed = document.querySelector<HTMLInputElement>('[data-scramble-seed]')
const turnLayer = document.querySelector<HTMLInputElement>('[data-turn-layer]')
const turnWide = document.querySelector<HTMLInputElement>('[data-turn-wide]')
const solveDepth = document.querySelector<HTMLInputElement>('[data-solve-depth]')
const importArea = document.querySelector<HTMLTextAreaElement>('[data-import-area]')
const importFile = document.querySelector<HTMLInputElement>('[data-import-file]')
const statusOrder = document.querySelector<HTMLElement>('[data-status-order]')
const statusMoves = document.querySelector<HTMLElement>('[data-status-moves]')
const statusTimer = document.querySelector<HTMLElement>('[data-status-timer]')
const statusHistory = document.querySelector<HTMLElement>('[data-status-history]')
const solverStatus = document.querySelector<HTMLElement>('[data-solver-status]')
const solverDetail = document.querySelector<HTMLElement>('[data-solver-detail]')
const solveButton = document.querySelector<HTMLButtonElement>('[data-action="solve"]')
const cancelSolveButton = document.querySelector<HTMLButtonElement>('[data-action="cancel-solve"]')
const stageCanvas = document.querySelector<HTMLCanvasElement>('#rubik-canvas')
const telemetryToggles = document.querySelectorAll<HTMLButtonElement>('.stage-toggle')
const telemetry = document.querySelector<HTMLElement>('.telemetry')
const layout = document.querySelector<HTMLElement>('.layout')

for (const toggle of telemetryToggles) {
  toggle.addEventListener('click', () => {
    telemetry?.classList.toggle('collapsed')
    layout?.classList.toggle('collapsed-telemetry')
    requestAnimationFrame(() => {
      window.dispatchEvent(new Event('resize'))
    })
  })
}

stageCanvas?.addEventListener('contextmenu', (event) => {
  event.preventDefault()
})

const suppressSyntheticMouseFromTouch = (event: TouchEvent) => {
  event.preventDefault()
}

for (const eventName of ['touchstart', 'touchmove', 'touchend', 'touchcancel'] as const) {
  stageCanvas?.addEventListener(eventName, suppressSyntheticMouseFromTouch, {
    passive: false,
  })
}

stageCanvas?.addEventListener(
  'wheel',
  (event) => {
    event.preventDefault()
  },
  { passive: false }
)

function formatTimer(elapsedMillis: number): string {
  const totalTenths = Math.floor(elapsedMillis / 100)
  const minutes = Math.floor(totalTenths / 600)
  const seconds = Math.floor((totalTenths % 600) / 10)
  const tenths = totalTenths % 10
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}.${tenths}`
}

function parseTurnHistoryLength(raw: string): number {
  try {
    const history = JSON.parse(raw) as unknown[]
    return Array.isArray(history) ? history.length : 0
  } catch (error) {
    console.error(error)
    return 0
  }
}

function parseScrambleSeed(raw: string | undefined): bigint {
  const trimmed = raw?.trim()
  if (!trimmed) {
    return BigInt(Date.now())
  }

  try {
    return BigInt(trimmed)
  } catch (error) {
    console.error(error)
    return BigInt(Date.now())
  }
}

function parseRuntimeStatus(raw: string): RuntimeStatus | null {
  try {
    return JSON.parse(raw) as RuntimeStatus
  } catch (error) {
    console.error(error)
    return null
  }
}

function renderRuntimeStatus(status: RuntimeStatus | null): void {
  if (!status) {
    return
  }

  statusOrder!.textContent = `${status.order}x${status.order}`
  statusMoves!.textContent = String(status.move_count)
  statusTimer!.textContent = formatTimer(status.elapsed_millis)
  statusHistory!.textContent = status.recent_turns.length > 0 ? status.recent_turns.join('  ·  ') : '—'

  if (orderSelect && orderSelect.value !== String(status.order)) {
    orderSelect.value = String(status.order)
  }

  if (bootCopy) {
    bootCopy.textContent = status.last_message
  }
}

function syncStatus(): void {
  if (!runtime) {
    return
  }

  const status = parseRuntimeStatus(runtime.runtime_status_json())
  if (!status) {
    return
  }

  if (
    activeSolveRequestId !== null &&
    activeSolveSceneRevision !== null &&
    status.scene_revision !== activeSolveSceneRevision
  ) {
    cancelActiveSolve('Direct runtime input changed the cube state and cancelled the in-flight solve request.')
  }

  renderRuntimeStatus(status)
}

function updateBootState(tone: BootTone, label: string, detail: string): void {
  bootPill?.setAttribute('data-tone', tone)
  if (bootPill) {
    bootPill.textContent = label
  }
  if (bootCopy) {
    bootCopy.textContent = detail
  }
}

function updateSolverState(label: string, detail: string): void {
  if (solverStatus) {
    solverStatus.textContent = label
  }
  if (solverDetail) {
    solverDetail.textContent = detail
  }
}

function solveDepthCap(order: number): number {
  if (order <= 2) {
    return 8
  }
  if (order === 3) {
    return 7
  }
  if (order <= 5) {
    return 5
  }
  return 4
}

function currentTurnSelection(order: number): { startLayer: number; width: number } {
  const requestedLayer = Math.min(order, Math.max(1, Number.parseInt(turnLayer?.value ?? '1', 10) || 1))
  if (turnLayer) {
    turnLayer.value = String(requestedLayer)
  }

  const startLayer = requestedLayer - 1
  const width = turnWide?.checked && requestedLayer < order ? 2 : 1
  return { startLayer, width }
}

function syncTurnControls(order: number): void {
  if (turnLayer) {
    turnLayer.max = String(order)
    const requestedLayer = Math.min(order, Math.max(1, Number.parseInt(turnLayer.value || '1', 10) || 1))
    turnLayer.value = String(requestedLayer)
  }

  if (turnWide) {
    turnWide.disabled = order < 3
    if (order < 3) {
      turnWide.checked = false
    }
  }
}

function syncSolveControls(): void {
  const order = Number.parseInt(orderSelect?.value ?? '3', 10) || 3
  const cap = solveDepthCap(order)
  const busy = activeSolveRequestId !== null

  if (solveDepth) {
    solveDepth.max = String(cap)
    const nextValue = Math.min(cap, Math.max(1, Number.parseInt(solveDepth.value || String(cap), 10) || cap))
    solveDepth.value = String(nextValue)
    solveDepth.disabled = busy
  }

  if (solveButton) {
    solveButton.disabled = busy
  }

  if (cancelSolveButton) {
    cancelSolveButton.disabled = !busy
  }

  syncTurnControls(order)
}

function currentSceneRevision(): number | null {
  if (!runtime) {
    return null
  }

  return parseRuntimeStatus(runtime.runtime_status_json())?.scene_revision ?? null
}

function activeSolveLaneCount(): number {
  return Math.max(1, Math.min(parallelLanes, rootFaceCodes.length))
}

function partitionRootFaces(laneCount: number): number[][] {
  const groups = Array.from({ length: laneCount }, () => [] as number[])

  rootFaceCodes.forEach((faceCode, index) => {
    groups[index % laneCount].push(faceCode)
  })

  return groups.filter((group) => group.length > 0)
}

function terminateSolverWorkers(): void {
  for (const worker of solverWorkers) {
    worker.terminate()
  }
  solverWorkers = []
}

function ensureSolverWorkers(count: number): Worker[] {
  while (solverWorkers.length < count) {
    solverWorkers.push(new Worker(new URL('./solver.worker.ts', import.meta.url), { type: 'module' }))
  }

  while (solverWorkers.length > count) {
    solverWorkers.pop()?.terminate()
  }

  return solverWorkers
}

function runSolveLane(worker: Worker, request: SolveWorkerRequest): Promise<SolveWorkerResponse> {
  return new Promise((resolve, reject) => {
    const cleanup = (): void => {
      worker.removeEventListener('message', onMessage)
      worker.removeEventListener('error', onError)
    }

    const onMessage = (event: MessageEvent<SolveWorkerResponse>): void => {
      if (event.data.requestId !== request.requestId || event.data.laneId !== request.laneId) {
        return
      }

      cleanup()
      resolve(event.data)
    }

    const onError = (): void => {
      cleanup()
      reject(new Error(`solver worker lane ${request.laneId + 1} crashed`))
    }

    worker.addEventListener('message', onMessage)
    worker.addEventListener('error', onError)
    worker.postMessage(request)
  })
}

function finishSolveSession(): void {
  activeSolveRequestId = null
  activeSolveSceneRevision = null
  syncSolveControls()
}

function cancelActiveSolve(detail: string): void {
  if (activeSolveRequestId === null) {
    return
  }

  finishSolveSession()
  terminateSolverWorkers()
  updateSolverState('cancelled', detail)
}

async function startSolve(module: RubikWasmModule): Promise<void> {
  if (activeSolveRequestId !== null) {
    updateSolverState('busy', 'A solve request is already running. Cancel it before starting another one.')
    return
  }

  const runtimeStatus = parseRuntimeStatus(module.runtime_status_json())
  const order = runtimeStatus?.order ?? (Number.parseInt(orderSelect?.value ?? '3', 10) || 3)
  if (runtimeStatus?.is_solved) {
    updateSolverState('already solved', 'The current state is already solved, so no worker search was started.')
    syncSolveControls()
    return
  }

  const cap = solveDepthCap(order)
  const maxDepth = Math.min(cap, Math.max(1, Number.parseInt(solveDepth?.value ?? '5', 10) || 5))
  if (solveDepth) {
    solveDepth.value = String(maxDepth)
  }

  const requestId = ++nextSolveRequestId
  activeSolveRequestId = requestId
  activeSolveSceneRevision = currentSceneRevision()
  syncSolveControls()
  const stateJson = module.export_cube_state()
  const turnHistoryJson = module.export_turn_history_json()
  const turnHistoryLength = parseTurnHistoryLength(turnHistoryJson)
  let totalExplored = 0

  if (order !== 3 && turnHistoryLength > 0) {
    const worker = ensureSolverWorkers(1)[0]
    updateSolverState(
      'solving',
      `Replaying the inverse of ${turnHistoryLength} recorded turn(s) in a Rust wasm worker for ${order}x${order}.`
    )

    let result: SolveWorkerResponse
    try {
      result = await runSolveLane(worker, {
        kind: 'solve',
        requestId,
        laneId: 0,
        order,
        stateJson,
        targetDepth: 1,
        allowedFaces: [...rootFaceCodes],
        turnHistoryJson,
      })
    } catch (error) {
      if (activeSolveRequestId !== requestId) {
        return
      }

      finishSolveSession()
      terminateSolverWorkers()
      updateSolverState('worker fault', error instanceof Error ? error.message : 'unknown worker history failure')
      return
    }

    if (activeSolveRequestId !== requestId) {
      return
    }

    const sceneRevision = currentSceneRevision()
    if (activeSolveSceneRevision !== null && sceneRevision !== activeSolveSceneRevision) {
      finishSolveSession()
      updateSolverState(
        'stale result discarded',
        'The cube state changed while the worker was replaying recorded history, so the returned solution was ignored.'
      )
      return
    }

    if (result.kind === 'error') {
      finishSolveSession()
      terminateSolverWorkers()
      updateSolverState('worker error', result.message)
      return
    }

    if (result.kind === 'solved') {
      finishSolveSession()

      const notation = result.turns.map((turn) => turn.notation).join(' ')
      for (const turn of result.turns) {
        module.apply_turn(turn.faceCode, turn.rotationCode, turn.startLayer, turn.width)
      }
      syncStatus()

      const suffix =
        result.turns.length > 0
          ? ` Applied ${result.turns.length} recorded inverse turn(s)${notation ? `: ${notation}.` : '.'}`
          : ' No turns were needed.'
      updateSolverState('solved', `${result.message}.${suffix}`)
      return
    }

    finishSolveSession()
    updateSolverState(
      'history unavailable',
      'Recorded history could not solve the current state, so this NxN request still needs a deeper feasible solver.'
    )
    return
  }

  const faceGroups = partitionRootFaces(activeSolveLaneCount())
  const workers = ensureSolverWorkers(faceGroups.length)

  for (let depth = 1; depth <= maxDepth; depth += 1) {
    if (activeSolveRequestId !== requestId) {
      return
    }

    updateSolverState(
      'solving',
      `Searching depth ${depth}/${maxDepth} for ${order}x${order} across ${faceGroups.length} worker lane(s).`
    )

    let results: SolveWorkerResponse[]
    try {
      results = await Promise.all(
        faceGroups.map((allowedFaces, laneId) =>
          runSolveLane(workers[laneId], {
            kind: 'solve',
            requestId,
            laneId,
            order,
            stateJson,
            targetDepth: depth,
            allowedFaces,
          })
        )
      )
    } catch (error) {
      if (activeSolveRequestId !== requestId) {
        return
      }

      finishSolveSession()
      terminateSolverWorkers()
      updateSolverState('worker fault', error instanceof Error ? error.message : 'unknown worker pool failure')
      return
    }

    if (activeSolveRequestId !== requestId) {
      return
    }

    totalExplored += results.reduce((sum, result) => sum + result.explored, 0)

    const sceneRevision = currentSceneRevision()
    if (activeSolveSceneRevision !== null && sceneRevision !== activeSolveSceneRevision) {
      finishSolveSession()
      updateSolverState(
        'stale result discarded',
        'The cube state changed while the worker pool was searching, so the returned solution was ignored.'
      )
      return
    }

    const errorResult = results.find((result) => result.kind === 'error')
    if (errorResult) {
      finishSolveSession()
      terminateSolverWorkers()
      updateSolverState('worker error', errorResult.message)
      return
    }

    const solvedResult = results.find((result) => result.kind === 'solved')
    if (solvedResult) {
      finishSolveSession()

      const notation = solvedResult.turns.map((turn) => turn.notation).join(' ')
      for (const turn of solvedResult.turns) {
        module.apply_turn(turn.faceCode, turn.rotationCode, turn.startLayer, turn.width)
      }
      syncStatus()

      const suffix =
        solvedResult.turns.length > 0
          ? ` Applied ${solvedResult.turns.length} turn(s) from the worker pool${notation ? `: ${notation}.` : '.'}`
          : ' No turns were needed.'
      updateSolverState(
        'solved',
        `${solvedResult.message}. Explored ${totalExplored.toLocaleString()} nodes across ${faceGroups.length} lane(s).${suffix}`
      )
      return
    }

    updateSolverState(
      'searching next depth',
      `Depth ${depth} finished with no solution. Explored ${totalExplored.toLocaleString()} nodes across ${faceGroups.length} lane(s) so far.`
    )
  }

  if (activeSolveRequestId !== requestId) {
    return
  }

  if (order === 3) {
    updateSolverState(
      'fallback solving',
      `Depth ${maxDepth} search finished unsolved. Escalating to a Rust two-phase 3x3 fallback in a dedicated worker.`
    )

    let fallbackResult: SolveWorkerResponse
    try {
      fallbackResult = await runSolveLane(ensureSolverWorkers(1)[0], {
        kind: 'solve',
        requestId,
        laneId: 0,
        order,
        stateJson,
        targetDepth: 0,
        allowedFaces: [],
      })
    } catch (error) {
      if (activeSolveRequestId !== requestId) {
        return
      }

      finishSolveSession()
      terminateSolverWorkers()
      updateSolverState('worker fault', error instanceof Error ? error.message : 'unknown 3x3 fallback failure')
      return
    }

    if (activeSolveRequestId !== requestId) {
      return
    }

    const sceneRevision = currentSceneRevision()
    if (activeSolveSceneRevision !== null && sceneRevision !== activeSolveSceneRevision) {
      finishSolveSession()
      updateSolverState(
        'stale result discarded',
        'The cube state changed while the 3x3 fallback worker was searching, so the returned solution was ignored.'
      )
      return
    }

    if (fallbackResult.kind === 'solved') {
      finishSolveSession()

      const notation = fallbackResult.turns.map((turn) => turn.notation).join(' ')
      for (const turn of fallbackResult.turns) {
        module.apply_turn(turn.faceCode, turn.rotationCode, turn.startLayer, turn.width)
      }
      syncStatus()

      const suffix =
        fallbackResult.turns.length > 0
          ? ` Applied ${fallbackResult.turns.length} fallback turn(s)${notation ? `: ${notation}.` : '.'}`
          : ' No turns were needed.'
      updateSolverState('solved', `${fallbackResult.message}.${suffix}`)
      return
    }

    if (fallbackResult.kind === 'error') {
      finishSolveSession()
      terminateSolverWorkers()
      updateSolverState('worker error', fallbackResult.message)
      return
    }
  }

  finishSolveSession()
  updateSolverState(
    'depth limit reached',
    `No solution was found up to depth ${maxDepth}. Explored ${totalExplored.toLocaleString()} nodes across ${faceGroups.length} lane(s).`
  )
}

function bindShellControls(module: RubikWasmModule): void {
  for (const button of document.querySelectorAll<HTMLButtonElement>('[data-action]')) {
    button.addEventListener('click', () => {
      switch (button.dataset.action) {
        case 'reset':
          cancelActiveSolve('Reset cancelled the in-flight solve request.')
          module.reset_cube()
          break
        case 'undo':
          cancelActiveSolve('Undo cancelled the in-flight solve request.')
          module.undo_turn()
          break
        case 'redo':
          cancelActiveSolve('Redo cancelled the in-flight solve request.')
          module.redo_turn()
          break
        case 'scramble': {
          cancelActiveSolve('Scrambling cancelled the in-flight solve request.')
          const length = Number.parseInt(scrambleLength?.value ?? '20', 10) || 20
          const seed = parseScrambleSeed(scrambleSeed?.value)
          module.scramble_cube(length, seed)
          break
        }
        case 'export': {
          const payload = module.export_cube_state()
          if (importArea) {
            importArea.value = payload
          }
          const blob = new Blob([payload], { type: 'application/json' })
          const url = URL.createObjectURL(blob)
          const anchor = document.createElement('a')
          anchor.href = url
          anchor.download = 'rubikrs-state.json'
          anchor.click()
          URL.revokeObjectURL(url)
          break
        }
        case 'import':
          if (importArea?.value.trim()) {
            cancelActiveSolve('Importing a new state cancelled the in-flight solve request.')
            module.import_cube_state(importArea.value.trim())
          }
          break
        case 'import-file':
          importFile?.click()
          break
        case 'solve':
          void startSolve(module)
          break
        case 'cancel-solve':
          cancelActiveSolve('Solve request cancelled. A fresh worker will be created next time.')
          break
      }

      syncStatus()
    })
  }

  for (const button of document.querySelectorAll<HTMLButtonElement>('[data-turn]')) {
    button.addEventListener('click', () => {
      const encoded = button.dataset.turn
      if (!encoded) {
        return
      }

      const [faceCode, rotationCode] = encoded.split(':').map((value) => Number.parseInt(value, 10))
      const order = Number.parseInt(orderSelect?.value ?? '3', 10) || 3
      const { startLayer, width } = currentTurnSelection(order)
      cancelActiveSolve('Manual turns cancelled the in-flight solve request.')
      module.apply_turn(faceCode, rotationCode, startLayer, width)
      syncStatus()
    })
  }

  orderSelect?.addEventListener('change', () => {
    cancelActiveSolve('Changing the cube order cancelled the in-flight solve request.')
    const nextOrder = Number.parseInt(orderSelect.value, 10)
    module.set_cube_order(nextOrder)
    syncSolveControls()
    syncStatus()
  })

  importFile?.addEventListener('change', async () => {
    const file = importFile.files?.[0]
    if (!file) {
      return
    }

    const text = await file.text()
    if (importArea) {
      importArea.value = text
    }

    cancelActiveSolve('Importing a state file cancelled the in-flight solve request.')
    module.import_cube_state(text)
    importFile.value = ''
    syncStatus()
  })

  syncSolveControls()
}

async function bootstrapRuntime(): Promise<void> {
  updateBootState('booting', 'loading wasm', 'Fetching the generated Bevy runtime package.')

  try {
    runtime = (await import('./generated/rubik_app/rubik_app.js')) as RubikWasmModule
    await runtime.default()

    updateBootState('booting', 'starting bevy', 'Binding the runtime to #rubik-canvas.')
    runtime.start_app('rubik-canvas', basePath)
    bindShellControls(runtime)
    ensureSolverWorkers(activeSolveLaneCount())
    syncStatus()

    if (statusPollHandle !== null) {
      window.clearInterval(statusPollHandle)
    }

    statusPollHandle = window.setInterval(() => {
      syncStatus()
    }, 100)

    updateBootState(
      'ready',
      'runtime live',
      'Bevy is rendering a live NxN sticker state. The shell now controls order changes, turns, import/export, scramble, undo, and redo.'
    )
  } catch (error) {
    console.error(error)

    const detail = error instanceof Error ? error.message : 'Unknown bootstrap error'
    updateBootState('fault', 'boot fault', detail)
  }
}

window.addEventListener('beforeunload', () => {
  terminateSolverWorkers()
})

void bootstrapRuntime()
