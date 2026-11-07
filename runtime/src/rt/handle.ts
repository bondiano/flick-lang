// Handlers of effects (docs/formal/core.md): deep, found dynamically in the
// evidence. An effect's implementation is a record of curried operations. A
// `tail` arm is a direct call, run with the evidence of the place of the
// handler. An `abort` arm throws to the handler and runs there. A `control` arm
// captures the frames between the operation and the handler: it holds them
// through `resume`, which puts them back once; if it ends without doing so, they
// are discontinued, a failure thrown into them that closes their scopes.
import { type Cause, Raise, Signal, causeOf, dies, fail, hasDies, hasFails, raise, signalOf, unwinds } from "./cause.ts";
import {
  type Ctx,
  type Fiber,
  type Frame,
  Instruction,
  type Run,
  Under,
  currentCtx,
  currentFiber,
  extend,
  inCtx,
  isRun,
  lookup,
  runIn,
  skipHandler,
} from "./fiber.ts";

/** spike: set by `bridge.ts`, so that this module stays without Effect. */
export const bridgeHooks: {
  operation: ((tag: object) => unknown) | undefined;
  ran: (value: unknown) => unknown;
} = { operation: undefined, ran: (value) => value };

/** An operation of a handler: its name, arm, class, and how many dictionaries and arguments it takes. */
export type OpInstall = readonly [
  name: string,
  arm: unknown,
  operation: "tail" | "abort" | "control",
  dicts: number,
  arity: number,
];

/** One effect a handler installs. */
export type Install = readonly [tag: object, operations: ReadonlyArray<OpInstall>];

/** A service's tag: an object of its own, compared by identity. */
export const service = (key: string): Readonly<{ key: string }> => Object.freeze({ key });

/** `f a` for a curried function of the program. */
const apply = (f: unknown, a: unknown): unknown => {
  if (typeof f !== "function") throw new TypeError("the function takes fewer arguments");
  const result: unknown = Reflect.apply(f, undefined, [a]);
  return result;
};

const applyAll = (f: unknown, args: ReadonlyArray<unknown>): unknown => args.reduce(apply, f);

/** The function of `arity` arguments that gives `f` all of them at once, after its dictionaries. */
const collected = (dicts: number, arity: number, f: (given: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>) => unknown): unknown => {
  const collect = (given: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>): unknown =>
    args.length === arity ? f(given, args) : (a: unknown) => collect(given, [...args, a]);
  return dicts === 0 ? collect([], []) : (...given: ReadonlyArray<unknown>) => collect(given, []);
};

const dictsApplied = (f: unknown, given: ReadonlyArray<unknown>): unknown => {
  if (given.length === 0) return f;
  if (typeof f !== "function") throw new TypeError("the operation takes no dictionaries");
  const applied: unknown = Reflect.apply(f, undefined, given);
  return applied;
};

/**
 * `S.op`: a function of the operation's arguments that looks the implementation
 * up when the last one is given, in the evidence at that moment. What it gives
 * is a value, or a call for the caller to run.
 */
export const operation = (tag: object, name: string, arity: number, dicts: ReadonlyArray<unknown> = []): unknown => {
  const call = (args: ReadonlyArray<unknown>): unknown => {
    const impl = lookup(tag);
    if (impl === undefined) {
      // spike: a service Effect's side provided, under the same key
      if (bridgeHooks.operation !== undefined) {
        const found = bridgeHooks.operation(tag);
        if (found !== undefined) return bridgeHooks.ran(applyAll(dictsApplied(Reflect.get(Object(found), name), dicts), args));
      }
      throw new Error(`no handler of \`${String(Reflect.get(tag, "key"))}\` is in effect`);
    }
    return applyAll(dictsApplied(Reflect.get(Object(impl), name), dicts), args);
  };
  const collect = (args: ReadonlyArray<unknown>): unknown =>
    args.length === arity ? call(args) : (a: unknown) => collect([...args, a]);
  return arity === 0 ? call([]) : collect([]);
};

/** A failure of a `tail` arm is the handler's, not that of the body in between. */
const armFailure = (thrown: unknown, token: object): unknown => {
  const cause = causeOf(thrown);
  if (hasFails(cause) && !hasDies(cause) && !unwinds(cause)) {
    return new Raise(fail(new Signal(token, undefined, cause, false)));
  }
  return thrown;
};

function* tailRun(run: unknown, token: object): Run {
  try {
    return yield run;
  } catch (thrown) {
    throw armFailure(thrown, token);
  }
}

/** The frames of a body between an operation and its handler, out of the stack. */
class Segment {
  readonly top: Frame;
  readonly bottom: Frame;
  /** The mask depth inside them when they were captured. */
  readonly mask: number;
  used = false;
  constructor(top: Frame, bottom: Frame, mask: number) {
    this.top = top;
    this.bottom = bottom;
    this.mask = mask;
  }
}

/** A handler while it runs. */
class Prompt {
  readonly token: object = {};
  readonly fiber: Fiber;
  readonly ctx: Ctx;
  /** The mask depth to return to when the body ends: the handler's, or the resuming arm's. */
  mask: number;
  constructor(fiber: Fiber, ctx: Ctx) {
    this.fiber = fiber;
    this.ctx = ctx;
    this.mask = fiber.mask;
  }
}

/** A `control` operation: captures up to its handler's body and runs the arm on what is below. */
class Control extends Instruction {
  private readonly prompt: Prompt;
  private readonly arm: unknown;
  constructor(prompt: Prompt, arm: unknown) {
    super();
    this.prompt = prompt;
    this.arm = arm;
  }
  exec(fiber: Fiber): boolean {
    if (fiber !== this.prompt.fiber) {
      throw new Error("a `control` operation ran in a fiber other than its handler's");
    }
    const top = fiber.top;
    let bottom = top;
    while (bottom !== null && bottom.prompt !== this.prompt) {
      if (bottom.barrier) throw new Error("a `control` operation ran under a stage that runs at once");
      bottom = bottom.prev;
    }
    if (top === null || bottom === null) throw new Error("a `control` operation ran after its handler ended");
    const segment = new Segment(top, bottom, fiber.mask);
    fiber.top = bottom.prev;
    bottom.prev = null;
    fiber.mask = this.prompt.mask;
    const { arm } = this;
    return new Under(this.prompt.ctx, () => armRun(this.prompt, segment, arm)).exec(fiber);
  }
}

/** Puts a segment back on top of the arm that resumes it. */
class Resume extends Instruction {
  private readonly prompt: Prompt;
  private readonly segment: Segment;
  private readonly value: unknown;
  constructor(prompt: Prompt, segment: Segment, value: unknown) {
    super();
    this.prompt = prompt;
    this.segment = segment;
    this.value = value;
  }
  exec(fiber: Fiber): boolean {
    if (this.segment.used) throw new Error("`resume` ran twice: the rest of a computation resumes once");
    this.segment.used = true;
    this.prompt.mask = fiber.mask;
    this.segment.bottom.prev = fiber.top;
    fiber.top = this.segment.top;
    fiber.mask = this.segment.mask;
    fiber.value = this.value;
    return false;
  }
}

/** Throws the discontinuing into a segment no one will resume: it unwinds to the handler's body, which ends. */
class Discontinue extends Instruction {
  private readonly prompt: Prompt;
  private readonly segment: Segment;
  constructor(prompt: Prompt, segment: Segment) {
    super();
    this.prompt = prompt;
    this.segment = segment;
  }
  exec(fiber: Fiber): boolean {
    this.segment.used = true;
    this.prompt.mask = fiber.mask;
    this.segment.bottom.prev = fiber.top;
    fiber.top = this.segment.top;
    fiber.mask = this.segment.mask;
    fiber.mode = 1;
    fiber.value = new Raise(fail(new Signal(this.prompt.token, undefined, undefined, true)));
    return false;
  }
}

function* armRun(prompt: Prompt, segment: Segment, arm: unknown): Run {
  const resume = (value: unknown): Instruction => new Resume(prompt, segment, value);
  let out: unknown;
  try {
    out = yield apply(arm, resume);
  } catch (thrown) {
    if (!segment.used) {
      try {
        yield new Discontinue(prompt, segment);
      } catch {
        // the failure of the arm is what the handler ends with
      }
    }
    throw thrown;
  }
  if (!segment.used) yield new Discontinue(prompt, segment);
  return out;
}

/** The frame the body of a handler starts at: how its end, whichever, makes the handler's value. */
function* bodyRun(
  prompt: Prompt,
  body: () => unknown,
  ret: ((value: unknown) => unknown) | undefined,
  errors: ((error: unknown) => (() => unknown) | undefined) | undefined,
): Run {
  let value: unknown;
  const fiber = prompt.fiber;
  try {
    value = yield body();
  } catch (thrown) {
    fiber.mask = prompt.mask;
    const cause = causeOf(thrown);
    const signal = signalOf(cause, prompt.token);
    if (signal !== undefined) {
      if (signal.discontinue) return undefined;
      if (signal.arm !== undefined) return new Under(prompt.ctx, signal.arm);
      return raise(signal.cause ?? cause);
    }
    if (errors === undefined || unwinds(cause)) throw thrown;
    return new Under(prompt.ctx, () => recovered(cause, errors));
  }
  fiber.mask = prompt.mask;
  if (ret === undefined) return value;
  return new Under(prompt.ctx, () => ret(value));
}

/**
 * What a `try` does with a cause: a defect turns the failures into defects, the
 * first failure goes to the arm that matches, and otherwise the cause goes on.
 */
const recovered = (cause: Cause, handler: (error: unknown) => (() => unknown) | undefined): unknown => {
  skipHandler(currentFiber(), cause);
  if (unwinds(cause)) return raise(cause);
  if (hasDies(cause)) return raise(hasFails(cause) ? dies(cause) : cause);
  for (const r of cause.reasons) {
    if (r._tag === "Fail") {
      const handled = handler(r.error);
      if (handled !== undefined) return handled();
      break;
    }
  }
  return raise(cause);
};

/** `try body with arms`, in a generator. */
export function* tryWith(body: () => unknown, handler: (error: unknown) => (() => unknown) | undefined): Run {
  let value: unknown;
  try {
    value = yield body();
  } catch (thrown) {
    return recovered(causeOf(thrown), handler);
  }
  return value;
}

/** `try body with arms` in a stage that runs at once: `body` runs on the stack. */
export const trySync = (body: () => unknown, handler: (error: unknown) => (() => unknown) | undefined): unknown => {
  try {
    return body();
  } catch (thrown) {
    return runIn(recovered(causeOf(thrown), handler));
  }
};

/** An arm of `installs` as the implementation of its operation. */
const armOf = (prompt: Prompt, [, arm, kind, dicts, arity]: OpInstall): unknown => {
  const { token, ctx } = prompt;
  const called = (given: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>): unknown =>
    applyAll(dictsApplied(arm, given), args);
  if (kind === "control") {
    return collected(dicts, arity, (given, args) => new Control(prompt, called(given, args)));
  }
  if (kind === "abort") {
    return collected(dicts, arity, (given, args) => {
      return raise(fail(new Signal(token, () => called(given, args), undefined, false)));
    });
  }
  return collected(dicts, arity, (given, args) => {
    let out: unknown;
    try {
      out = inCtx(ctx, () => called(given, args));
    } catch (thrown) {
      throw armFailure(thrown, token);
    }
    return isRun(out) || out instanceof Instruction ? new Under(ctx, () => tailRun(out, token)) : out;
  });
};

/**
 * The handler of `installs` around `body`: `ret` makes its value of the body's,
 * and `errors` handles the body's typed errors as the arms of a `try` do; their
 * values are the handler's.
 */
export const handle = (
  installs: ReadonlyArray<Install>,
  body: () => unknown,
  ret: ((value: unknown) => unknown) | undefined,
  errors: ((error: unknown) => (() => unknown) | undefined) | undefined,
): Under => {
  const ctx = currentCtx();
  const prompt = new Prompt(currentFiber(), ctx);
  let inner = ctx;
  for (const [tag, operations] of installs) {
    const impl: Record<string, unknown> = {};
    for (const op of operations) impl[op[0]] = armOf(prompt, op);
    inner = extend(inner, tag, impl);
  }
  return new Under(inner, () => bodyRun(prompt, body, ret, errors), prompt);
};
