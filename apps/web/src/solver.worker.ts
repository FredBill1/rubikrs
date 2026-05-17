type SolveWorkerRequest = {
  kind: 'solve'
  requestId: number
  order: number
  stateJson: string
  maxDepth: number
}

type SolveWorkerResponse = {
  kind: 'solved' | 'unsolved' | 'error'
  requestId: number
  turns: Array<{
    faceCode: number
    rotationCode: number
    startLayer: number
    width: number
    notation: string
  }>
  explored: number
  depthLimit: number
  message: string
}

type SolverWorkerModule = {
  default: () => Promise<unknown>
  solve_state_json: (stateJson: string, maxDepth: number) => string
}

let modulePromise: Promise<SolverWorkerModule> | null = null

async function ensureSolverModule(): Promise<SolverWorkerModule> {
  if (!modulePromise) {
    modulePromise = import('./generated/solver_worker/solver_worker.js').then(
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
  if (event.data.kind !== 'solve') {
    return
  }

  try {
    const module = await ensureSolverModule()
    const response = JSON.parse(module.solve_state_json(event.data.stateJson, event.data.maxDepth)) as Omit<
      SolveWorkerResponse,
      'requestId'
    >

    const payload: SolveWorkerResponse = {
      ...response,
      requestId: event.data.requestId,
    }

    self.postMessage(payload)
  } catch (error) {
    self.postMessage({
      kind: 'error',
      requestId: event.data.requestId,
      turns: [],
      explored: 0,
      depthLimit: event.data.maxDepth,
      message: error instanceof Error ? error.message : 'unknown solver worker error',
    } satisfies SolveWorkerResponse)
  }
})

export {}
