// Predicates are plain functions; these combinators preserve short circuiting.
type Predicate<A> = (value: A) => boolean;
export const mapInput = <A, B>(f: (value: B) => A) => (predicate: Predicate<A>): Predicate<B> =>
  (value) => predicate(f(value));
export const not = <A>(predicate: Predicate<A>): Predicate<A> => (value) => !predicate(value);
export const and = <A>(that: Predicate<A>) => (self: Predicate<A>): Predicate<A> =>
  (value) => self(value) && that(value);
export const or = <A>(that: Predicate<A>) => (self: Predicate<A>): Predicate<A> =>
  (value) => self(value) || that(value);
export const xor = <A>(that: Predicate<A>) => (self: Predicate<A>): Predicate<A> =>
  (value) => self(value) !== that(value);
export const eqv = <A>(that: Predicate<A>) => (self: Predicate<A>): Predicate<A> =>
  (value) => self(value) === that(value);
export const implies = <A>(that: Predicate<A>) => (self: Predicate<A>): Predicate<A> =>
  (value) => !self(value) || that(value);
export const nand = <A>(that: Predicate<A>) => (self: Predicate<A>): Predicate<A> =>
  (value) => !(self(value) && that(value));
export const nor = <A>(that: Predicate<A>) => (self: Predicate<A>): Predicate<A> =>
  (value) => !(self(value) || that(value));
export const every = <A>(predicates: ReadonlyArray<Predicate<A>>): Predicate<A> =>
  (value) => predicates.every((predicate) => predicate(value));
export const some = <A>(predicates: ReadonlyArray<Predicate<A>>): Predicate<A> =>
  (value) => predicates.some((predicate) => predicate(value));
