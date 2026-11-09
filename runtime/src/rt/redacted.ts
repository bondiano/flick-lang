// Values stay in a weak map, so ordinary inspection and JSON serialization
// cannot reveal them. `reveal` is the explicit escape hatch.
const contents = new WeakMap<object, unknown>();
export const redactedHooks: { reveal?: (value: unknown) => { readonly value: unknown } | undefined } = {};

export interface Redacted<A> {
  readonly toString: () => string;
  readonly toJSON: () => string;
  readonly _value?: A;
}

export const redactedMake = <A>(value: A): Redacted<A> => {
  const wrapper = Object.freeze({
    toString: () => "<redacted>",
    toJSON: () => "<redacted>",
  });
  contents.set(wrapper, value);
  return wrapper;
};

export const redactedReveal = <A>(redacted: Redacted<A>): A => {
  if (!contents.has(redacted)) {
    const hosted = redactedHooks.reveal?.(redacted);
    if (hosted === undefined) throw new TypeError("not a Redacted value");
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the optional adapter preserves Redacted's value type
    return hosted.value as A;
  }
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- only redactedMake puts a value under this wrapper
  return contents.get(redacted) as A;
};

export const flickEquivalent = <A>(
  isEquivalent: (self: A, that: A) => boolean,
  self: Redacted<A>,
  that: Redacted<A>,
): boolean => isEquivalent(redactedReveal(self), redactedReveal(that));
