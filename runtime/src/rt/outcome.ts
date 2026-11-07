// Outcomes as Flick sees them: `Exit`, `Failure` and `RawCause` keep a cause
// opaque.
import { type Cause, type Exit, recoverable } from "./cause.ts";
import { none, type Option, some } from "../core/data.ts";

/** `Failure e`: how a computation failed. */
export class Failure {
  readonly cause: Cause;
  /** Whether it is the interruption of a closing owner. */
  readonly shutdown: boolean;
  constructor(cause: Cause, shutdown = false) {
    this.cause = cause;
    this.shutdown = shutdown;
    Object.freeze(this);
  }
}

/** `RawCause`: the whole cause, for diagnostics only. */
export class RawCause {
  readonly cause: Cause;
  constructor(cause: Cause) {
    this.cause = cause;
    Object.freeze(this);
  }
}

export const exitValue = (exit: Exit): Option<unknown> => (exit.cause === undefined ? some(exit.value) : none);
export const exitFailure = (exit: Exit): Option<Failure> => (exit.cause === undefined ? none : some(new Failure(exit.cause)));

/** `Failure.primary`: the typed error recovery would select. */
export const failurePrimary = (failure: Failure): Option<unknown> => {
  const reason = recoverable(failure.cause);
  return reason === undefined ? none : some(reason.error);
};

export const failureRawCause = (failure: Failure): RawCause => new RawCause(failure.cause);
