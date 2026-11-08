// Runs a build in a worker, so a long or endless program can be stopped.
import { execute, type Message, type Program } from "./runtime.ts";

onmessage = (event: MessageEvent<Program>) => execute(event.data, (message: Message) => postMessage(message));
