// The preview of a program with an interface. It runs in an iframe whose sandbox
// allows scripts and nothing else, so its origin is opaque: the program, which may
// come from someone else's link, cannot reach the page, its storage or the network.
import { execute, type Message, type Program } from "./runtime.ts";

if (self.origin !== "null" || parent === self) throw new Error("The preview runs only in the playground's sandboxed frame");

const post = (message: Message): void => parent.postMessage(message, "*");

let started = false;
addEventListener("message", (event: MessageEvent<Program>) => {
  if (event.source !== parent || started) return;
  started = true;
  void execute(event.data, post);
});
post({ stream: "ready" });
