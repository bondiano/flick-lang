// The bindings of `Layer` on the kernel, as Go's `flickrt` has Effect's.
import { type Cause, die, dies, isFail, raise } from "../cause.ts";
import { type Run, Suspend, type Under, asExit, currentOwner, exitOf } from "../fiber.ts";
import {
  type Build,
  Layer,
  MemoMap,
  type Services,
  buildWith,
  constant,
  currentMemo,
  fromBuild,
  memoizable,
  mergeAll,
  scoped,
  servicesOf,
  withServices,
} from "../layer.ts";
import { implementation } from "../handle.ts";
import { Failure } from "../outcome.ts";
import { type Owner, withOwner } from "../owner.ts";

export const empty: Layer = /*#__PURE__*/ constant([]);

export const succeedContext = (services: Services): Layer => constant(services);

export const sync =
  (tag: object) =>
  (make: (unit: undefined) => unknown): Layer =>
    memoizable(() => [[tag, make(undefined)]]);

export const syncContext = (make: (unit: undefined) => unknown): Layer => memoizable(() => make(undefined));

/** `Layer.effect tag task`: the implementation `task` computes when the layer builds. */
export const effect =
  (tag: object) =>
  (task: () => unknown): Layer =>
    memoizable(function* (_memo, into): Run {
      return [[tag, yield scoped(into, task)]];
    });

export const effectContext = (task: () => unknown): Layer => memoizable((_memo, into) => scoped(into, task));

export const effectDiscard = (task: () => unknown): Layer =>
  memoizable(function* (_memo, into): Run {
    yield scoped(into, task);
    return [];
  });

export const suspend = (make: (unit: undefined) => Layer): Layer => memoizable((memo, into) => make(undefined).build(memo, into));

export const unwrap = (task: () => unknown): Layer =>
  fromBuild(function* (memo, into): Run {
    const layer = layerOf(yield scoped(into, task));
    return yield layer.build(memo, into);
  });

export const flickMergeAll = (first: Layer, rest: ReadonlyArray<Layer>): Layer => mergeAll([first, ...rest]);

const layerOf = (value: unknown): Layer => {
  if (value instanceof Layer) return value;
  throw new TypeError("not a layer");
};

/** `Layer.provide_ that self`: `self` built with the services of `that`. */
const provideTo =
  (that: Layer) =>
  (self: Layer): Layer =>
    fromBuild(function* (memo, into): Run {
      const inputs = servicesOf(yield that.build(memo, into));
      return yield withServices(inputs, () => self.build(memo, into));
    });
export { provideTo as provide };

export const provideMerge =
  (that: Layer) =>
  (self: Layer): Layer =>
    fromBuild(function* (memo, into): Run {
      const inputs = servicesOf(yield that.build(memo, into));
      return [...inputs, ...servicesOf(yield withServices(inputs, () => self.build(memo, into)))];
    });

export const flatMap =
  (f: (services: Services) => Layer) =>
  (self: Layer): Layer =>
    fromBuild(function* (memo, into): Run {
      const services = servicesOf(yield self.build(memo, into));
      return yield f(services).build(memo, into);
    });

export const tap =
  (f: (services: Services) => unknown) =>
  (self: Layer): Layer =>
    fromBuild(function* (memo, into): Run {
      const services = servicesOf(yield self.build(memo, into));
      yield scoped(into, () => f(services));
      return services;
    });

/** A layer that runs `handle` on the cause `self` fails with: the layer it gives is built in its place; none, and it fails as it did. */
const onFailure = (self: Layer, handle: (into: Owner, cause: Cause) => unknown): Layer =>
  fromBuild(function* (memo, into): Run {
    const built = asExit(yield exitOf(() => self.build(memo, into)));
    if (built.cause === undefined) return built.value;
    const next: unknown = yield handle(into, built.cause);
    if (next === undefined) return raise(built.cause);
    return yield layerOf(next).build(memo, into);
  });

/** The error of the first typed failure of `cause`, if it has one. */
const firstError = (cause: Cause): readonly [unknown] | undefined => {
  const reason = cause.reasons.find(isFail);
  return reason === undefined ? undefined : [reason.error];
};

export const tapError =
  (f: (error: unknown) => unknown) =>
  (self: Layer): Layer =>
    onFailure(self, function* (into, cause): Run {
      const error = firstError(cause);
      if (error !== undefined) yield scoped(into, () => f(error[0]));
      return undefined;
    });

export const flickTapCause =
  (f: (failure: Failure) => unknown) =>
  (self: Layer): Layer =>
    onFailure(self, function* (into, cause): Run {
      yield scoped(into, () => f(new Failure(cause)));
      return undefined;
    });

const catchIt =
  (f: (error: unknown) => Layer) =>
  (self: Layer): Layer =>
    onFailure(self, (_into, cause) => {
      const error = firstError(cause);
      return error === undefined ? undefined : f(error[0]);
    });
export { catchIt as catch };

export const flickCatchCause =
  (f: (failure: Failure) => Layer) =>
  (self: Layer): Layer =>
    onFailure(self, (_into, cause) => f(new Failure(cause)));

export const orDie = (self: Layer): Layer =>
  new Layer(function* (memo, into): Run {
    const built = asExit(yield exitOf(() => self.build(memo, into)));
    return built.cause === undefined ? built.value : raise(dies(built.cause));
  });

/** `self` built with the service of `tag` its input has changed by `f`. */
export const updateService = (tag: object, f: (impl: unknown) => unknown) => (self: Layer): Layer =>
  fromBuild((memo, into) => {
    const impl = implementation(tag);
    if (impl === undefined) return raise(die(`Service not found: ${String(Reflect.get(tag, "key"))}`));
    return withServices([[tag, f(impl)]], () => self.build(memo, into));
  });

export const fresh = (self: Layer): Layer => new Layer((_memo, into) => self.build(new MemoMap(undefined), into));

/** Builds the layer and holds its services until interrupted. */
export const launch = (self: Layer): Run =>
  withOwner(function* (owner: Owner): Run {
    yield buildWith(self, currentMemo(), owner);
    return yield new Suspend(() => undefined);
  });

/** The owner of the call: the innermost `scope`'s. */
const here = (): Owner => {
  const owner = currentOwner();
  if (owner === undefined) throw new Error("a layer is built outside any scope");
  return owner;
};

export const build = (self: Layer): Under => buildWith(self, currentMemo(), here());

export const flickBuildWithMemoMap =
  (memo: MemoMap) =>
  (self: Layer): Under =>
    buildWith(self, memo, here());

export const flickMakeMemoMap = (): MemoMap => new MemoMap(undefined);
export const forkMemoMap = (parent: MemoMap): MemoMap => new MemoMap(parent);
export const flickCurrentMemoMap = (): MemoMap => currentMemo();

/** The build of `make`, a function of the memo map to a task that acquires into the layer's scope. */
const fromTask =
  (make: (memo: MemoMap) => unknown): Build =>
  (memo, into) =>
    scoped(into, () => make(memo));

export const flickFromBuild = (make: (memo: MemoMap) => unknown): Layer => fromBuild(fromTask(make));
export const flickFromBuildMemo = (make: (memo: MemoMap) => unknown): Layer => memoizable(fromTask(make));
