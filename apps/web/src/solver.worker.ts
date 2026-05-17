type SolveWorkerRequest = {
  kind: 'solve'
  requestId: number
  laneId: number
  order: number
  stateJson: string
  targetDepth: number
  allowedFaces: number[]
}

type SolveWorkerResponse = {
  kind: 'solved' | 'unsolved' | 'error'
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
  depthLimit: number
  message: string
}

type SolverWorkerModule = {
  default: () => Promise<unknown>
  solve_request_json: (requestJson: string) => string
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
    const response = JSON.parse(module.solve_request_json(JSON.stringify({
      stateJson: event.data.stateJson,
      targetDepth: event.data.targetDepth,
      allowedFaces: event.data.allowedFaces,
    }))) as Omit<
      SolveWorkerResponse,
      'requestId' | 'laneId'
    >

    const payload: SolveWorkerResponse = {
      ...response,
      requestId: event.data.requestId,
      laneId: event.data.laneId,
    }

    self.postMessage(payload)
  } catch (error) {
    self.postMessage({
      kind: 'error',
      requestId: event.data.requestId,
      laneId: event.data.laneId,
      turns: [],
      explored: 0,
      depthLimit: event.data.targetDepth,
      message: error instanceof Error ? error.message : 'unknown solver worker error',
    } satisfies SolveWorkerResponse)
  }
})

export {}
