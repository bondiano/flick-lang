// Inputs that JSON text cannot spell, and a Standard Schema caller.
export const cyclic = () => {
  const node = { _tag: "Branch", _0: [] };
  node._0.push(node);
  return node;
};
export const shared = () => {
  const leaf = { _tag: "Leaf", _0: 1 };
  return { _tag: "Branch", _0: [leaf, leaf] };
};
export const notANumber = () => NaN;
export const validate = (standard, value) =>
  Promise.resolve(standard["~standard"].validate(value)).then((r) =>
    r.issues ? `issues: ${r.issues[0].message}` : `value: ${JSON.stringify(r.value)}`,
  );
