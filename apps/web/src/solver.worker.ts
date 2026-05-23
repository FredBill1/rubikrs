type SolveWorkerRequest = {
  kind: 'solve' | 'cancel'
  requestId?: number
  laneId?: number
  order?: number
  stateJson?: string
}

type SolveWorkerResponse = {
  kind: 'solved' | 'unsolved' | 'error' | 'cancelled'
  requestId: number
  laneId: number
  turns: Array<{
    faceCode: number
    rotationCode: number
    startLayer: number
    width: number
    notation: string
  }>
  explored: number
  message: string
}

type SolverWorkerModule = {
  default: () => Promise<unknown>
  solve_request_json: (requestJson: string) => string
  request_cancel_solver: () => void
}

let modulePromise: Promise<SolverWorkerModule> | null = null
let activeRequestId: number | null = null
let activeLaneId: number | null = null

async function ensureSolverModule(): Promise<SolverWorkerModule> {
  if (!modulePromise) {
    modulePromise = import('./generated/rubik_solver/rubik_solver.js').then(
      async (module) => {
        const typedModule = module as SolverWorkerModule
        await typedModule.default()
        return typedModule
      }
    )
  }

  return modulePromise
}

self.addEventListener('message', async (event: MessageEvent<SolveWorkerRequest>) => {
  const data = event.data

  if (data.kind === 'cancel') {
    // Signal cancellation to the Rust solver
    try {
      const module = await ensureSolverModule()
      module.request_cancel_solver()
    } catch {
      // Module may not be loaded yet - that's OK
    }
    if (activeRequestId !== null && activeLaneId !== null) {
      self.postMessage({
        kind: 'cancelled',
        requestId: activeRequestId,
        laneId: activeLaneId,
        turns: [],
        explored: 0,
        message: 'Solve request cancelled.',
      } satisfies SolveWorkerResponse)
    }
    return
  }

  if (data.kind !== 'solve') {
    return
  }

  if (data.requestId === undefined || data.laneId === undefined || !data.stateJson) {
    self.postMessage({
      kind: 'error',
      requestId: data.requestId ?? 0,
      laneId: data.laneId ?? 0,
      turns: [],
      explored: 0,
      message: 'Invalid solve request: missing requestId, laneId, or stateJson',
    } satisfies SolveWorkerResponse)
    return
  }

  activeRequestId = data.requestId
  activeLaneId = data.laneId

  try {
    const module = await ensureSolverModule()
    const response = JSON.parse(module.solve_request_json(JSON.stringify({
      stateJson: data.stateJson,
    }))) as Omit<
      SolveWorkerResponse,
      'requestId' | 'laneId'
    >

    const payload: SolveWorkerResponse = {
      ...response,
      requestId: data.requestId,
      laneId: data.laneId,
    }

    self.postMessage(payload)
  } catch (error) {
    self.postMessage({
      kind: 'error',
      requestId: data.requestId,
      laneId: data.laneId,
      turns: [],
      explored: 0,
      message: error instanceof Error ? error.message : 'unknown solver worker error',
    } satisfies SolveWorkerResponse)
  } finally {
    activeRequestId = null
    activeLaneId = null
  }
})

export {}
