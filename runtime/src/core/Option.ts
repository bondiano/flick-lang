// `Option` of the standard library without Effect: the bindings of Option.fl,
// with Effect's semantics. A binding with `arity k` takes its first k Flick
// parameters and returns the function of the option, as Effect's data-last form.
import { err, none, ok, type Option, type Result, some } from "./data.ts";

const map = <A, B>(self: Option<A>, f: (a: A) => B): Option<B> => (self._tag === "None" ? none : some(f(self.value)));

export const all = <A>(options: ReadonlyArray<Option<A>>): Option<Array<A>> => {
  const out: Array<A> = [];
  for (const o of options) {
    if (o._tag === "None") return none;
    out.push(o.value);
  }
  return some(out);
};

export const as =
  <B>(b: B) =>
  <A>(self: Option<A>): Option<B> =>
    map(self, () => b);

export const asVoid = /*#__PURE__*/ as(undefined);

const void_: Option<undefined> = /*#__PURE__*/ some(undefined);
export { void_ as void };

export const composeK =
  <B, C>(g: (b: B) => Option<C>) =>
  <A>(f: (a: A) => Option<B>) =>
  (a: A): Option<C> => {
    const b = f(a);
    return b._tag === "None" ? none : g(b.value);
  };

export const containsWith =
  <A>(same: (a: A, b: A) => boolean) =>
  (x: A) =>
  (self: Option<A>): boolean =>
    self._tag === "Some" && same(self.value, x);

export const exists =
  <A>(p: (a: A) => boolean) =>
  (self: Option<A>): boolean =>
    self._tag === "Some" && p(self.value);

export const filter =
  <A>(p: (a: A) => boolean) =>
  (self: Option<A>): Option<A> =>
    self._tag === "Some" && p(self.value) ? some(self.value) : none;

export const filterMap =
  <A, B, E>(f: (a: A) => Result<B, E>) =>
  (self: Option<A>): Option<B> => {
    if (self._tag === "None") return none;
    const r = f(self.value);
    return r._tag === "Success" ? some(r.success) : none;
  };

export const firstSomeOf = <A>(options: ReadonlyArray<Option<A>>): Option<A> =>
  options.find((o) => o._tag === "Some") ?? none;

export const flatten = <A>(self: Option<Option<A>>): Option<A> => (self._tag === "None" ? none : self.value);

export const zipWith =
  <A, B, C>(that: Option<B>, f: (a: A, b: B) => C) =>
  (self: Option<A>): Option<C> =>
    self._tag === "Some" && that._tag === "Some" ? some(f(self.value, that.value)) : none;

export const lift2 =
  <A, B, C>(f: (a: A, b: B) => C) =>
  (that: Option<B>): ((self: Option<A>) => Option<C>) =>
    zipWith(that, f);

export const liftPredicate =
  <A>(p: (a: A) => boolean) =>
  (a: A): Option<A> =>
    p(a) ? some(a) : none;

export const match =
  <A, B>(cases: { readonly onNone: () => B; readonly onSome: (a: A) => B }) =>
  (self: Option<A>): B =>
    self._tag === "None" ? cases.onNone() : cases.onSome(self.value);

export const orElseResult =
  <B>(that: () => Option<B>) =>
  <A>(self: Option<A>): Option<Result<B, A>> =>
    self._tag === "None" ? map(that(), ok) : some(err(self.value));

export const orElseSome =
  <A>(fallback: () => A) =>
  (self: Option<A>): Option<A> =>
    self._tag === "None" ? some(fallback()) : self;

export const partitionMap =
  <A, B, D>(f: (a: A) => Result<D, B>) =>
  (self: Option<A>): [Option<D>, Option<B>] => {
    if (self._tag === "None") return [none, none];
    const r = f(self.value);
    return r._tag === "Failure" ? [none, some(r.failure)] : [some(r.success), none];
  };

export const product =
  <B>(that: Option<B>) =>
  <A>(self: Option<A>): Option<[A, B]> =>
    self._tag === "Some" && that._tag === "Some" ? some([self.value, that.value]) : none;

export const productMany =
  <A>(others: ReadonlyArray<Option<A>>) =>
  (self: Option<A>): Option<Array<A>> => {
    if (self._tag === "None") return none;
    const out = [self.value];
    for (const o of others) {
      if (o._tag === "None") return none;
      out.push(o.value);
    }
    return some(out);
  };

export const reduceCompact =
  <A, B>(b: B, f: (b: B, a: A) => B) =>
  (options: ReadonlyArray<Option<A>>): B =>
    options.reduce((acc, o) => (o._tag === "Some" ? f(acc, o.value) : acc), b);

export const tap =
  <A, X>(f: (a: A) => Option<X>) =>
  (self: Option<A>): Option<A> =>
    self._tag === "Some" && f(self.value)._tag === "Some" ? some(self.value) : none;

export const zipLeft =
  <X>(that: Option<X>) =>
  <A>(self: Option<A>): Option<A> =>
    self._tag === "Some" && that._tag === "Some" ? some(self.value) : none;

export const zipRight =
  <B>(that: Option<B>) =>
  <X>(self: Option<X>): Option<B> =>
    self._tag === "None" ? none : that;
