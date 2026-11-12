// Preloaded by `flick run --devtools` on Flick's own runtime (`node
// --import`): the DevTools at `FLICK_DEVTOOLS` get the spans of the program.
// A module of its own, so no built program carries the client.
import { devTools } from "../devtools.ts";
import { env } from "../host.ts";

devTools(env("FLICK_DEVTOOLS") ?? "ws://localhost:34437");
