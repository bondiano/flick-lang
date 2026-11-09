// A program that imports only `Effect` from the runtime.
import { Effect } from "@flick-lang/effect-ts";

Effect.runFork(Effect.log("hi"));
