// The two places where the runtime knows more than TypeScript can: an index
// a caller checked against the length, and a function of the program whose
// argument type the compiler checked. Everything else is narrowed by checks.

/** `xs[i]` for an index in range by the caller's check; `A` may itself include `undefined`. */
export const item = <A>(xs: ReadonlyArray<A>, i: number): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- in range by the caller's check
  xs[i] as A;

/**
 * A function of the program, called with values a schema or codec decoded:
 * they have the type the compiler checked the function against.
 */
export const erase = <B>(f: (a: never) => B): ((a: unknown) => B) =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the argument type
  f as (a: unknown) => B;
