export class WorkerHarness {
  static instances: WorkerHarness[] = [];
  readonly messages: Array<{ id: number; action: string; source?: string }> = [];
  onmessage: ((event: { data: { id: number; result: unknown } }) => void) | undefined;
  onerror: ((event: { error: Error; message: string }) => void) | undefined;
  terminated = false;

  constructor() { WorkerHarness.instances.push(this); }
  postMessage(message: { id: number; action: string; source?: string }): void { this.messages.push(message); }
  terminate(): void { this.terminated = true; }
  reply(id: number, result: unknown): void { this.onmessage?.({ data: { id, result } }); }
  crash(): void { this.onerror?.({ error: new Error("compiler offline"), message: "compiler offline" }); }
}

