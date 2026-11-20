// Handlers of effects (docs/formal/core.md): deep, found dynamically in the
// evidence. An effect's implementation is a record of curried operations. A
// `tail` arm is a direct call, run with the evidence of the place of the
// handler. An `abort` arm throws to the handler and runs there. A `control` arm
// captures the frames between the operation and the handler: it holds them
// through `resume`, which puts them back once; if it ends without doing so, they
// are discontinued, a failure thrown into them that closes their scopes.
import {
  type Cause,
  type Exit,
  Raise,
  Signal,
  causeOf,
  elect,
  fail,
  failed,
  fromReasons,
  hasFails,
  hasInterrupts,
  hasRealDie,
  raise,
  signalOf,
  stripSignals,
  succeeded,
  unwinds,
} from "./cause.ts";
import {
  type Ctx,
  type Fiber,
  type Frame,
  Instruction,
  type Run,
  Under,
  asExit,
  currentCtx,
  currentFiber,
  exitOf,
  extend,
  evidenced,
  inCtx,
  isRun,
  lookup,
  runIn,
  skipHandler,
} from "./fiber.ts";

/** Filled by the optional Effect adapter when generated code crosses into it. */
export const bridgeHooks: {
  operation: ((tag: object) => unknown) | undefined;
  ran: (value: unknown) => unknown;
  identity?: (fiber: Fiber) => number;
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

// Only effect-handler installs are partial. A layer or a raw service record
// remains a complete provider, even when a malformed record omits an operation.
const partialImplementations = /*#__PURE__*/ new WeakSet<object>();

/** Internal bridge metadata: this record supplies only its own operation arms. */
export const isPartialImplementation = (impl: unknown): impl is object =>
  typeof impl === "object" && impl !== null && partialImplementations.has(impl);

/** The nearest provider of this operation, not merely of its effect's tag. */
const operationImplementation = (tag: object, name: string): unknown => {
  for (let ctx: Ctx | null = currentCtx(); ctx !== null; ctx = ctx.next) {
    if (ctx.tag !== tag) continue;
    if (isPartialImplementation(ctx.impl) && !Object.hasOwn(ctx.impl, name)) continue;
    return ctx.impl;
  }
  return undefined;
};

const tags = /*#__PURE__*/ new Map<string, Readonly<{ key: string }>>();

/**
 * A service's tag, compared by identity: one per key, `Module.Service`, so
 * every lookup of a key gives the same tag and the runtime reaches the
 * services of `Atom` and `Ui` that a program declares.
 */
export const service = (key: string): Readonly<{ key: string }> => {
  let tag = tags.get(key);
  if (tag === undefined) {
    tag = Object.freeze({ key });
    tags.set(key, tag);
  }
  return tag;
};

/** `f a` for a curried function of the program. */
const apply = (f: unknown, a: unknown): unknown => {
  if (typeof f !== "function") throw new TypeError("the function takes fewer arguments");
  const result: unknown = Reflect.apply(f, undefined, [a]);
  return result;
};

const applyAll = (f: unknown, args: ReadonlyArray<unknown>): unknown => args.reduce(apply, f);

// Handler wrappers know their terminal runtime stage. The bridge must defer its
// invocation, not inspect an already invoked arm: that arm can capture context.
const operationEntries = /*#__PURE__*/ new WeakSet<object>();
export const isOperationEntry = (value: unknown): boolean =>
  typeof value === "function" && operationEntries.has(value);

/** The function of `arity` arguments that gives `f` all of them at once, after its dictionaries. */
const collected = (dicts: number, arity: number, f: (given: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>) => unknown): unknown => {
  const collect = (given: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>): unknown => {
    if (args.length === arity) return f(given, args);
    const next = (a: unknown): unknown => collect(given, [...args, a]);
    if (args.length + 1 === arity) operationEntries.add(next);
    return next;
  };
  return dicts === 0 ? collect([], []) : (...given: ReadonlyArray<unknown>) => collect(given, []);
};

const dictsApplied = (f: unknown, given: ReadonlyArray<unknown>): unknown => {
  if (given.length === 0) return f;
  if (typeof f !== "function") throw new TypeError("the operation takes no dictionaries");
  const applied: unknown = Reflect.apply(f, undefined, given);
  return applied;
};

const invoke = (impl: unknown, name: string, dicts: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>): unknown =>
  applyAll(dictsApplied(Reflect.get(Object(impl), name), dicts), args);

/** `S.op` given its last argument: the implementation `impl` the evidence has for it now, applied. */
const performed = (impl: unknown, tag: object, name: string, dicts: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>): unknown => {
  if (impl === undefined) {
    const fromHost = bridgeHooks.operation?.(tag);
    if (fromHost !== undefined) return bridgeHooks.ran(invoke(fromHost, name, dicts, args));
    throw new Error(`no handler of \`${String(Reflect.get(tag, "key"))}\` is in effect`);
  }
  if (impl instanceof Served) return impl.call(name, dicts, args);
  return invoke(impl, name, dicts, args);
};

/**
 * `S.op`: a function of the operation's arguments that looks the implementation
 * up when the last one is given, in the evidence at that moment. What it gives
 * is a value, or a call for the caller to run. One of one argument and no
 * dictionaries, the most common, collects nothing into an array.
 */
export const operation = (tag: object, name: string, arity: number, dicts: ReadonlyArray<unknown> = []): unknown => {
  if (arity === 1 && dicts.length === 0) {
    return (a: unknown): unknown => {
      const impl = operationImplementation(tag, name);
      if (impl === undefined || impl instanceof Served) return performed(impl, tag, name, dicts, [a]);
      return apply(Reflect.get(Object(impl), name), a);
    };
  }
  const collect = (args: ReadonlyArray<unknown>): unknown =>
    args.length === arity ? performed(operationImplementation(tag, name), tag, name, dicts, args) : (a: unknown) => collect([...args, a]);
  return collect([]);
};

/** A failure of a `tail` arm is the handler's, not that of the body in between. */
const armFailure = (thrown: unknown, token: object): unknown => {
  const cause = causeOf(thrown);
  if (hasFails(cause) && !hasRealDie(cause) && !unwinds(cause)) {
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

/** A `tail` arm's call: run with the evidence of its handler, a failure the handler's. */
const tailCall = (prompt: Prompt, called: () => unknown): unknown => {
  const { token } = prompt;
  const ctx = evidenced(prompt.ctx, currentCtx().stageOwner);
  let out: unknown;
  try {
    out = inCtx(ctx, called);
  } catch (thrown) {
    throw armFailure(thrown, token);
  }
  return isRun(out) || out instanceof Instruction ? new Under(ctx, () => tailRun(out, token)) : out;
};

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

/** How many handlers have started: each one's number orders them. */
let prompts = 0;

/** A handler while it runs. */
class Prompt {
  readonly token: object = {};
  readonly seq: number = ++prompts;
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

const wrongFiber = (kind: string): never => {
  throw new Error(`${kind} operation ran in a fiber other than its handler's`);
};

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
    if (fiber !== this.prompt.fiber) wrongFiber("a `control`");
    if (this.prompt.seq <= fiber.cut) {
      fiber.mode = 1;
      fiber.value = new Raise(fail(cut));
      return false;
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
    return new Under(evidenced(this.prompt.ctx, currentCtx().stageOwner), () => armRun(this.prompt, segment, arm)).exec(fiber);
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

/** What a cut-off `control` operation throws: it unwinds the cleanup and adds nothing to the cause. */
const cut = /*#__PURE__*/ new Signal({}, undefined, undefined, true);

/**
 * How `cleanup` ends after a body that ended with `exit`. A body that was
 * interrupted, or unwinds to a handler, did not end normally: a `control`
 * operation of a handler older than the cleanup is then cut off, its arm does
 * not run and the cleanup stops there, so the interruption or the unwind goes
 * on as it was (docs/formal/runtime.md, cleanup off the normal path).
 */
export function* cleanupAfter(exit: Exit, cleanup: () => unknown): Run {
  if (exit.cause === undefined || !(hasInterrupts(exit.cause) || unwinds(exit.cause))) return yield exitOf(cleanup);
  const fiber = currentFiber();
  const outer = fiber.cut;
  fiber.cut = prompts;
  try {
    const ended = asExit(yield exitOf(cleanup));
    if (ended.cause === undefined) return ended;
    const rest = ended.cause.reasons.filter((r) => !(r._tag === "Fail" && r.error === cut));
    return rest.length === 0 ? succeeded(undefined) : failed(fromReasons(rest));
  } finally {
    fiber.cut = outer;
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
  errors: ((error: unknown, unhandled: () => never) => (() => unknown) | undefined) | undefined,
): Run {
  let value: unknown;
  const fiber = prompt.fiber;
  try {
    value = yield body();
  } catch (thrown) {
    fiber.mask = prompt.mask;
    const cause = causeOf(thrown);
    if (hasRealDie(cause)) {
      if (errors === undefined) {
        const cleaned = stripSignals(cause);
        if (cleaned !== cause) return raise(cleaned);
        throw thrown;
      }
    } else {
      const signal = signalOf(cause, prompt.token);
      if (signal !== undefined) {
        if (signal.discontinue) return undefined;
        if (signal.arm !== undefined) return new Under(prompt.ctx, signal.arm);
        return raise(signal.cause ?? cause);
      }
      if (errors === undefined || unwinds(cause)) throw thrown;
    }
    return new Under(prompt.ctx, () => recovered(cause, errors));
  }
  fiber.mask = prompt.mask;
  if (ret === undefined) return value;
  return new Under(prompt.ctx, () => ret(value));
}

/**
 * What a `try` does with a cause: a defect turns the failures into defects, the
 * first failure goes to the arm that matches, and otherwise the cause goes on.
 * Deferred selectors retain `unhandled` to rethrow this exact cause after guards.
 */
const recovered = (cause: Cause, handler: (error: unknown, unhandled: () => never) => (() => unknown) | undefined): unknown => {
  skipHandler(currentFiber(), cause);
  if (hasRealDie(cause)) return raise(elect(cause));
  if (unwinds(cause)) return raise(cause);
  for (const r of cause.reasons) {
    if (r._tag === "Fail") {
      const handled = handler(r.error, () => raise(cause));
      if (handled !== undefined) return handled();
      break;
    }
  }
  return raise(cause);
};

/** `try body with arms`, in a generator. */
export function* tryWith(body: () => unknown, handler: (error: unknown, unhandled: () => never) => (() => unknown) | undefined): Run {
  let value: unknown;
  try {
    value = yield body();
  } catch (thrown) {
    return recovered(causeOf(thrown), handler);
  }
  return value;
}

/** `try body with arms` in a stage that runs at once: `body` runs on the stack. */
export const trySync = (body: () => unknown, handler: (error: unknown, unhandled: () => never) => (() => unknown) | undefined): unknown => {
  try {
    return body();
  } catch (thrown) {
    return runIn(recovered(causeOf(thrown), handler));
  }
};

/** An arm of `installs` as the implementation of its operation. */
const armOf = (prompt: Prompt, [, arm, kind, dicts, arity]: OpInstall): unknown => {
  const { token } = prompt;
  const called = (given: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>): unknown =>
    applyAll(dictsApplied(arm, given), args);
  if (kind === "control") {
    return collected(dicts, arity, (given, args) => new Control(prompt, called(given, args)));
  }
  if (kind === "abort") {
    return collected(dicts, arity, (given, args) => {
      const f = currentFiber();
      const id = bridgeHooks.identity;
      if (f !== prompt.fiber && (!id || id(f) !== id(prompt.fiber))) wrongFiber("an `abort`");
      const ctx = evidenced(prompt.ctx, currentCtx().stageOwner);
      return raise(fail(new Signal(token, () => new Under(ctx, () => called(given, args)), undefined, false)));
    });
  }
  return collected(dicts, arity, (given, args) => tailCall(prompt, () => called(given, args)));
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
  errors: ((error: unknown, unhandled: () => never) => (() => unknown) | undefined) | undefined,
): Under => {
  const ctx = currentCtx();
  const prompt = new Prompt(currentFiber(), ctx);
  let inner = ctx;
  for (const [tag, operations] of installs) {
    const impl = Object.fromEntries<unknown>(operations.map((op) => [op[0], armOf(prompt, op)]));
    partialImplementations.add(impl);
    inner = extend(inner, tag, impl);
  }
  return new Under(inner, () => bodyRun(prompt, body, ret, errors), prompt);
};

/**
 * A service a layer built, in effect as a handler: each of its operations is
 * a `tail` arm, which runs with the evidence of the place of the `provide`, as
 * the arms of `provide S impl` do (docs/formal/runtime.md §3.5).
 */
class Served {
  declare private readonly prompt: Prompt;
  declare readonly impl: unknown;
  constructor(prompt: Prompt, impl: unknown) {
    this.prompt = prompt;
    this.impl = impl;
  }
  call(name: string, given: ReadonlyArray<unknown>, args: ReadonlyArray<unknown>): unknown {
    return tailCall(this.prompt, () => invoke(this.impl, name, given, args));
  }
}

/** The implementation of `tag` in the evidence, whether a layer or a record gave it. */
export const implementation = (tag: object): unknown => {
  const impl = lookup(tag);
  return impl instanceof Served ? impl.impl : impl;
};

/** `body` under the handlers of `services` on top of `ctx`; their operations run with the evidence here. */
export const serving = (ctx: Ctx, services: ReadonlyArray<readonly [tag: object, impl: unknown]>, body: () => unknown): Under => {
  const prompt = new Prompt(currentFiber(), currentCtx());
  const inner = services.reduce((c: Ctx, [tag, impl]) => extend(c, tag, new Served(prompt, impl)), ctx);
  return new Under(inner, () => bodyRun(prompt, body, undefined, undefined), prompt);
};
