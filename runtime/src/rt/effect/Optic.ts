// Pure optics over Flick's plain data ABI. Each optic knows how to focus and
// rebuild a value; optional updates preserve the source on a failed focus.
import { err, ok, type Option, type Result } from "../../core/data.ts";
import { schemaError, type SchemaError } from "../schema.ts";

type Focus<S, A> = (source: S) => Result<A, string>;
type Update<S, A> = (value: A, source: S) => Result<S, string>;
interface Optic<S, A> {
  readonly focus: Focus<S, A>;
  readonly update: Update<S, A>;
  readonly build?: (value: A) => S;
}
const missing = <A>(message: string): Result<A, string> => err(message);
const make = <S, A>(focus: Focus<S, A>, update: Update<S, A>, build?: (value: A) => S): Optic<S, A> =>
  ({ focus, update, ...(build ? { build } : {}) });
const read = <S, A>(optic: Optic<S, A>, source: S): Result<A, SchemaError> => {
  const result = optic.focus(source);
  return result._tag === "Success" ? result : err(schemaError("", result.failure));
};
export const makeIso = <S, A>(get: (source: S) => A, reverse: (value: A) => S): Optic<S, A> =>
  make((source) => ok(get(source)), (value) => ok(reverse(value)), reverse);
export const makeLens = <S, A>(get: (source: S) => A, replace: (value: A, source: S) => S): Optic<S, A> =>
  make((source) => ok(get(source)), (value, source) => ok(replace(value, source)));
export const flickMakePrism = <S, A>(get: Focus<S, A>, build: (value: A) => S): Optic<S, A> =>
  make(get, (value) => ok(build(value)), build);
export const flickMakeOptional = <S, A>(get: Focus<S, A>, replace: (value: A, source: S) => Result<S, string>): Optic<S, A> =>
  make(get, replace);
export const flickFromCheck = <A>(check: (value: A) => boolean, message: string): Optic<A, A> =>
  flickMakePrism((value) => check(value) ? ok(value) : missing(message), (value) => value);
export const id = <A>(): Optic<A, A> => makeIso((value) => value, (value) => value);
export const some = <A>(): Optic<Option<A>, A> =>
  flickMakePrism((value) => value._tag === "Some" ? ok(value.value) : missing("expected a Some value"),
    (value) => ({ _tag: "Some", value }));
export const none = (): Optic<{ readonly _tag: string }, undefined> =>
  flickMakePrism((value) => value._tag === "None" ? ok(undefined) : missing("expected a None value"),
    () => ({ _tag: "None" }));
export const success = <A, E>(): Optic<Result<A, E>, A> =>
  flickMakePrism((value) => value._tag === "Success" ? ok(value.success) : missing("expected a Result.Success value"), ok);
export const failure = <A, E>(): Optic<Result<A, E>, E> =>
  flickMakePrism((value) => value._tag === "Failure" ? ok(value.failure) : missing("expected a Result.Failure value"), err);
export const get = <S, A>(optic: Optic<S, A>) => (source: S): A => {
  const result = optic.focus(source);
  if (result._tag === "Failure") throw new Error(result.failure);
  return result.success;
};
export const flickGetResult = <S, A>(optic: Optic<S, A>, source: S): Result<A, SchemaError> => read(optic, source);
export const set = <S, A>(optic: Optic<S, A>) => (value: A): S => {
  if (!optic.build) throw new Error("an optic cannot build a value");
  return optic.build(value);
};
export const replace = <S, A>(optic: Optic<S, A>, value: A) => (source: S): S => {
  const result = optic.focus(source);
  if (result._tag === "Failure") return source;
  const updated = optic.update(value, source);
  return updated._tag === "Success" ? updated.success : source;
};
export const flickReplaceResult = <S, A>(optic: Optic<S, A>, value: A, source: S): Result<S, SchemaError> => {
  const focused = read(optic, source);
  if (focused._tag === "Failure") return focused;
  const updated = optic.update(value, source);
  return updated._tag === "Success" ? updated : err(schemaError("", updated.failure));
};
export const modify = <S, A>(optic: Optic<S, A>, f: (value: A) => A) => (source: S): S => {
  const focused = optic.focus(source);
  return focused._tag === "Success" ? replace(optic, f(focused.success))(source) : source;
};
export const getAll = <S, A>(optic: Optic<S, ReadonlyArray<A>>) => (source: S): Array<A> => {
  const focused = optic.focus(source);
  return focused._tag === "Failure" ? [] : [...focused.success];
};
export const modifyAll = <S, A>(optic: Optic<S, ReadonlyArray<A>>, f: (value: A) => A) => (source: S): S => {
  const focused = optic.focus(source);
  if (focused._tag === "Failure") return source;
  return replace(optic, focused.success.map(f))(source);
};
export const flickCompose = <S, A, B>(inner: Optic<A, B>, outer: Optic<S, A>): Optic<S, B> => {
  const outerBuild = outer.build;
  const innerBuild = inner.build;
  const build = outerBuild && innerBuild ? (value: B): S => outerBuild(innerBuild(value)) : undefined;
  return make((source) => {
    const first = outer.focus(source);
    return first._tag === "Failure" ? first : inner.focus(first.success);
  }, (value, source) => {
    const first = outer.focus(source);
    if (first._tag === "Failure") return first;
    const second = inner.update(value, first.success);
    return second._tag === "Failure" ? second : outer.update(second.success, source);
  }, build);
};
export const flickAs = <A>(optic: A): A => optic;
