// The preview of a program with an interface. It runs in an iframe whose sandbox
// allows scripts and nothing else, so its origin is opaque: the program, which may
// come from someone else's link, cannot reach the page, its storage or the network.
import { execute, type Message, type Program } from "./runtime.ts";

if (self.origin !== "null" || parent === self) throw new Error("The preview runs only in the playground's sandboxed frame");

const post = (message: Message): void => parent.postMessage(message, "*");
const detail = (error: unknown): string => error instanceof Error ? (error.stack ?? error.message) : String(error);

const root = document.getElementById("app");
if (root === null) throw new Error("The preview has no app element");
const observer = new MutationObserver(() => {
  if (root.childNodes.length === 0) return;
  observer.disconnect();
  post({ stream: "mounted" });
});
observer.observe(root, { childList: true });

addEventListener("error", (event) => post({ stream: "crash", text: detail(event.error ?? event.message) }));
addEventListener("unhandledrejection", (event) => post({ stream: "crash", text: detail(event.reason) }));

let started = false;
addEventListener("message", (event: MessageEvent<Program>) => {
  if (event.source !== parent || started) return;
  started = true;
  void execute(event.data, post);
});
post({ stream: "ready" });
