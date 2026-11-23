// Runs a build in a worker, so a long or endless program can be stopped.
import { execute, type Message, type Program } from "./runtime.ts";

let controller: AbortController | undefined;
onmessage = (event: MessageEvent<Program | { readonly action: "stop" }>) => {
  if ("action" in event.data) {
    controller?.abort();
    return;
  }
  if (controller !== undefined) throw new Error("A program worker runs only one build");
  controller = new AbortController();
  return execute(event.data, (message: Message) => postMessage(message), controller.signal);
};
