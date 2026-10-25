// `Result` of the standard library without Effect: the bindings of Result.fl,
// with Effect's semantics, curried after their declared arity as Option.ts.
import { err, none, ok, type Option, type Result, some } from "./data.ts";

export const all = <A, E>(results: ReadonlyArray<Result<A, E>>): Result<Array<A>, E> => {
  const out: Array<A> = [];
  for (const r of results) {
    if (r._tag === "Failure") return r;
    out.push(r.success);
  }
  return ok(out);
};

export const failVoid: Result<never, undefined> = /*#__PURE__*/ err(undefined);

const void_: Result<undefined> = /*#__PURE__*/ ok(undefined);
export { void_ as void };

export const filterOrFail =
  <A, E>(p: (a: A) => boolean, orFailWith: (a: A) => E) =>
  (self: Result<A, E>): Result<A, E> =>
    self._tag === "Failure" ? self : p(self.success) ? ok(self.success) : err(orFailWith(self.success));

export const flip = <A, E>(self: Result<A, E>): Result<E, A> =>
  self._tag === "Failure" ? ok(self.failure) : err(self.success);

export const getFailure = <A, E>(self: Result<A, E>): Option<E> => (self._tag === "Success" ? none : some(self.failure));

export const liftPredicate =
  <A, E>(p: (a: A) => boolean, orFailWith: (a: A) => E) =>
  (a: A): Result<A, E> =>
    p(a) ? ok(a) : err(orFailWith(a));

export const mapBoth =
  <A, E, A2, E2>(f: { readonly onFailure: (e: E) => E2; readonly onSuccess: (a: A) => A2 }) =>
  (self: Result<A, E>): Result<A2, E2> =>
    self._tag === "Failure" ? err(f.onFailure(self.failure)) : ok(f.onSuccess(self.success));

export const match =
  <A, E, B>(cases: { readonly onFailure: (e: E) => B; readonly onSuccess: (a: A) => B }) =>
  (self: Result<A, E>): B =>
    self._tag === "Failure" ? cases.onFailure(self.failure) : cases.onSuccess(self.success);

export const merge = <A>(self: Result<A, A>): A => (self._tag === "Failure" ? self.failure : self.success);

export const orElse =
  <A, E, E2>(that: (e: E) => Result<A, E2>) =>
  (self: Result<A, E>): Result<A, E2> =>
    self._tag === "Failure" ? that(self.failure) : ok(self.success);

export const succeedNone: Result<Option<never>> = /*#__PURE__*/ ok(none);

export const succeedSome = <A>(a: A): Result<Option<A>> => ok(some(a));

const mapSome = <A, E>(r: Result<A, E>): Result<Option<A>, E> => (r._tag === "Failure" ? r : ok(some(r.success)));

export const transposeOption = <A, E>(self: Option<Result<A, E>>): Result<Option<A>, E> =>
  self._tag === "None" ? succeedNone : mapSome(self.value);

export const transposeMapOption =
  <A, B, E>(f: (a: A) => Result<B, E>) =>
  (self: Option<A>): Result<Option<B>, E> =>
    self._tag === "None" ? succeedNone : mapSome(f(self.value));
