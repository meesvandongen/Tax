// Main-thread handle to a simulation worker.

import type { Frame, FromWorker, MeshData, Metrics, Policy, Preset, SimEvent, ToWorker } from './protocol';

export interface ReadyInfo {
  width: number;
  height: number;
  heights: Float32Array;
  mesh: MeshData;
  overlays: string[];
  presets: Preset[];
  config: { seed: number; policy: Policy; [k: string]: unknown };
}

type Listener<T> = (v: T) => void;

export class SimClient {
  private worker: Worker;
  private inspectId = 0;
  private inspectWaiters = new Map<number, (d: unknown) => void>();
  readonly history: Metrics[] = [];
  onReady: Listener<ReadyInfo> = () => {};
  onFrame: Listener<{ frame: Frame; events: SimEvent[] }> = () => {};
  onMetrics: Listener<Metrics[]> = () => {};
  onPerf: Listener<number> = () => {};
  onError: Listener<string> = (m) => console.error(m);

  constructor(readonly role: 'main' | 'shadow') {
    this.worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromWorker>) => this.handle(e.data);
  }

  private send(msg: ToWorker) {
    this.worker.postMessage(msg);
  }

  private handle(msg: FromWorker) {
    switch (msg.type) {
      case 'ready':
        this.history.length = 0;
        this.onReady(msg);
        break;
      case 'metrics':
        this.history.push(...msg.metrics);
        this.onMetrics(msg.metrics);
        break;
      case 'frame':
        this.onFrame({ frame: msg.frame, events: msg.events });
        break;
      case 'perf':
        this.onPerf(msg.msPerMonth);
        break;
      case 'inspect': {
        const w = this.inspectWaiters.get(msg.id);
        this.inspectWaiters.delete(msg.id);
        w?.(msg.data);
        break;
      }
      case 'error':
        this.onError(msg.message);
        break;
    }
  }

  init(wasmUrl: string, config: unknown) {
    this.send({ type: 'init', wasmUrl, config, role: this.role });
  }
  reset(config: unknown) {
    this.send({ type: 'reset', config });
  }
  setPolicy(policy: Policy) {
    this.send({ type: 'setPolicy', policy });
  }
  run(monthsPerSecond: number) {
    this.send({ type: 'run', monthsPerSecond });
  }
  pause() {
    this.send({ type: 'pause' });
  }
  step(months: number) {
    this.send({ type: 'step', months });
  }
  inspect(cell: number): Promise<unknown> {
    const id = ++this.inspectId;
    return new Promise((resolve) => {
      this.inspectWaiters.set(id, resolve);
      this.send({ type: 'inspect', cell, id });
    });
  }
  terminate() {
    this.worker.terminate();
  }
}
