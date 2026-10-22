// Scopes and their children.
//
// shortcut: a Flick scope is a native Effect scope and children are forked
// into it; M8 replaces this with the lexical owner and two-phase close.
import { Cause, Effect, Exit } from "effect";

/** `scope [s] { body }`: `body` receives the owner and runs inside it. */
export const scope = (body) => Effect.scoped(Effect.flatMap(Effect.scope, (owner) => body(owner)));

/** `Fiber.spawn owner supervisor task`. */
export const fiberSpawn = (owner, supervisor, task) =>
  Effect.forkIn(
    Effect.onExit(task, (exit) =>
      Exit.isFailure(exit) && !Cause.hasInterruptsOnly(exit.cause)
        ? supervisor(exit.cause)
        : Effect.void,
    ),
    owner,
  );
