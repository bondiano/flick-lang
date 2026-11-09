// `Bool` of the standard library without Effect: the bindings of Bool.fl,
// with Effect's semantics; a binary connective takes `that`, then `self`.

/** Effect's `Reducer` shape, which `Reducer.fl` reads. */
export interface Reducer<A> {
  readonly combine: (self: A, that: A) => A;
  readonly initial: A;
  readonly initialValue: A;
  readonly combineAll: (xs: Iterable<A>) => A;
}

const reducer = <A>(combine: (self: A, that: A) => A, initialValue: A): Reducer<A> => ({
  combine,
  initial: initialValue,
  initialValue,
  combineAll: (xs) => {
    let out = initialValue;
    for (const x of xs) out = combine(out, x);
    return out;
  },
});

export const not = (self: boolean): boolean => !self;

const binary =
  (f: (self: boolean, that: boolean) => boolean) =>
  (that: boolean) =>
  (self: boolean): boolean =>
    f(self, that);

export const and = /*#__PURE__*/ binary((self, that) => self && that);
export const nand = /*#__PURE__*/ binary((self, that) => !(self && that));
export const or = /*#__PURE__*/ binary((self, that) => self || that);
export const nor = /*#__PURE__*/ binary((self, that) => !(self || that));
export const xor = /*#__PURE__*/ binary((self, that) => self !== that);
export const eqv = /*#__PURE__*/ binary((self, that) => self === that);
export const implies = /*#__PURE__*/ binary((self, that) => !self || that);

export const every = (xs: ReadonlyArray<boolean>): boolean => xs.every(Boolean);
export const some = (xs: ReadonlyArray<boolean>): boolean => xs.some(Boolean);

export const match =
  <A>(cases: { readonly onFalse: () => A; readonly onTrue: () => A }) =>
  (self: boolean): A =>
    self ? cases.onTrue() : cases.onFalse();

export const ReducerAnd: Reducer<boolean> = /*#__PURE__*/ reducer((a, b) => a && b, true);
export const ReducerOr: Reducer<boolean> = /*#__PURE__*/ reducer((a, b) => a || b, false);
