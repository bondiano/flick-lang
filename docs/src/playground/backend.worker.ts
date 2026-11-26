// Dedicated isolated Web Worker hosting the official `flick/js` WASM component.
// Loads `component.wasm` and its pinned `backend.json` lazily on the first emit
// request, verifies the SHA-256 before instantiation, and grants no authority
// beyond verified declared `inputs.read`.
import {
  BackendComponent,
  base64ToBytes,
  bytesToBase64,
  type BackendDiagnostic,
  type BackendPin,
  type BuildRequest,
  type BuildResult,
  type Descriptor,
} from "./backend-component.ts";

export interface BackendWorkerRequest {
  readonly id: number;
  readonly pin: BackendPin;
  readonly request: Omit<BuildRequest, "program"> & { readonly program: ReadonlyArray<number> };
  readonly program_b64: string;
  readonly inputs: Readonly<Record<string, string>>;
}

export type BackendWorkerPayload =
  | {
      readonly status: "ok";
      readonly descriptor: Descriptor;
      readonly result: Omit<BuildResult, "artifacts"> & {
        readonly artifacts: ReadonlyArray<Omit<BuildResult["artifacts"][number], "bytes"> & { readonly bytes: ReadonlyArray<number> }>;
      };
      readonly artifact_bytes_b64: ReadonlyArray<string>;
    }
  | {
      readonly status: "err";
      readonly descriptor: Descriptor;
      readonly diagnostics: ReadonlyArray<BackendDiagnostic>;
    }
  | {
      readonly status: "transport_error";
      readonly message: string;
    };

export type BackendWorkerReply =
  | { readonly id: number; readonly payload: BackendWorkerPayload }
  | { readonly id: number; readonly error: string };

let loaded: Promise<BackendComponent> | undefined;

const loadComponent = async (): Promise<BackendComponent> => {
  const [pinResponse, wasmResponse] = await Promise.all([
    fetch(new URL("./wasm/backend.json", import.meta.url)),
    fetch(new URL("./wasm/component.wasm", import.meta.url)),
  ]);
  if (!pinResponse.ok) {
    throw new Error(`Failed to fetch backend pin (${pinResponse.status})`);
  }
  if (!wasmResponse.ok) {
    throw new Error(`Failed to fetch backend component (${wasmResponse.status})`);
  }
  const pin = (await pinResponse.json()) as BackendPin;
  const bytes = new Uint8Array(await wasmResponse.arrayBuffer());
  return BackendComponent.load(bytes, pin);
};

const component = async (): Promise<BackendComponent> => {
  const current = loaded ??= loadComponent();
  try {
    return await current;
  } catch (error) {
    if (loaded === current) loaded = undefined;
    throw error;
  }
};

onmessage = async (event: MessageEvent<BackendWorkerRequest>) => {
  const message = event.data;
  try {
    const backend = await component();
    const request: BuildRequest = {
      ...message.request,
      program: base64ToBytes(message.program_b64),
    };
    const inputs = new Map<string, Uint8Array>();
    for (const [id, b64] of Object.entries(message.inputs)) {
      inputs.set(id, base64ToBytes(b64));
    }
    const checked = await backend.checkTarget(request, inputs);
    if (!checked.ok) {
      postMessage({
        id: message.id,
        payload: {
          status: "err",
          descriptor: backend.descriptor,
          diagnostics: checked.diagnostics,
        },
      } satisfies BackendWorkerReply);
      return;
    }
    const generated = await backend.generate(request, inputs);
    if (!generated.ok) {
      postMessage({
        id: message.id,
        payload: {
          status: "err",
          descriptor: backend.descriptor,
          diagnostics: generated.diagnostics,
        },
      } satisfies BackendWorkerReply);
      return;
    }
    const artifactBytes = generated.value.artifacts.map((artifact) => bytesToBase64(artifact.bytes));
    const wireResult = {
      ...generated.value,
      artifacts: generated.value.artifacts.map((artifact) => ({
        id: artifact.id,
        path: artifact.path,
        kind: artifact.kind,
        bytes: [],
      })),
    };
    postMessage({
      id: message.id,
      payload: {
        status: "ok",
        descriptor: backend.descriptor,
        result: wireResult,
        artifact_bytes_b64: artifactBytes,
      },
    } satisfies BackendWorkerReply);
  } catch (error) {
    postMessage({
      id: message.id,
      payload: {
        status: "transport_error",
        message: error instanceof Error ? error.message : String(error),
      },
    } satisfies BackendWorkerReply);
  }
};

onmessageerror = () => {
  postMessage({ id: 0, error: "The backend worker received an unreadable message" } satisfies BackendWorkerReply);
};
