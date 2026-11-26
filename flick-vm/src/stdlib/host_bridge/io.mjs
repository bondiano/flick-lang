// Dedicated TCP I/O thread for the counted Host/FFI worker.
// Keeps the Node main event loop completely non-blocking during async work
// while enabling synchronous re-entrant Flick callbacks via SharedArrayBuffer
// + Atomics.wait without fd/EAGAIN hazards or dropped lines.
import net from "node:net";
import { parentPort, workerData } from "node:worker_threads";

const { port, ctrlBuf, dataBuf } = workerData;
const ctrl = new Int32Array(ctrlBuf);
const data = new Uint8Array(dataBuf);
const MAX_CHUNK = data.byteLength;

// ctrl[0]:
//   0 = async/idle (deliver via parentPort.postMessage)
//   1 = main waiting for next sync line/chunk
//   2 = chunk ready in dataBuf for main
//   3 = socket closed
//   4 = main processing a delivered sync line (pause draining!)
// ctrl[1]: byte length in dataBuf
// ctrl[2]: 1 = more chunks follow for this line, 0 = final chunk of line

const socket = net.createConnection({ host: "127.0.0.1", port });
socket.setNoDelay(true);

let leftover = Buffer.alloc(0);
const pending = [];

function deliverSync(buf) {
  let offset = 0;
  do {
    const end = Math.min(offset + MAX_CHUNK, buf.length);
    const len = end - offset;
    const more = end < buf.length ? 1 : 0;
    data.set(buf.subarray(offset, end), 0);
    Atomics.store(ctrl, 1, len);
    Atomics.store(ctrl, 2, more);
    Atomics.store(ctrl, 0, 2);
    Atomics.notify(ctrl, 0, 1);
    while (Atomics.load(ctrl, 0) === 2) {
      Atomics.wait(ctrl, 0, 2);
    }
    offset = end;
  } while (offset < buf.length);
}

function drain() {
  while (pending.length > 0) {
    const state = Atomics.load(ctrl, 0);
    if (state === 1) {
      const buf = pending.shift();
      deliverSync(buf);
    } else if (state === 0) {
      const buf = pending.shift();
      parentPort.postMessage(buf.toString("utf8"));
    } else {
      // State 2, 3, or 4 (sync processing pause): stop draining until woken.
      break;
    }
  }
}

socket.on("data", (chunk) => {
  leftover = leftover.length === 0 ? chunk : Buffer.concat([leftover, chunk]);
  for (;;) {
    const nl = leftover.indexOf(0x0a);
    if (nl < 0) break;
    const line = leftover.subarray(0, nl);
    leftover = leftover.subarray(nl + 1);
    if (line.length > 0) {
      pending.push(Buffer.from(line));
    }
  }
  drain();
});

socket.on("close", () => {
  Atomics.store(ctrl, 0, 3);
  Atomics.notify(ctrl, 0, 1);
  parentPort.postMessage(null);
});

socket.on("error", () => {
  Atomics.store(ctrl, 0, 3);
  Atomics.notify(ctrl, 0, 1);
  parentPort.postMessage(null);
});

parentPort.on("message", (msg) => {
  if (msg === null) {
    socket.end();
    return;
  }
  if (typeof msg === "string") {
    socket.write(msg + "\n");
  }
  // Both string messages and `{ wake: true }` re-run `drain()`.
  drain();
});
