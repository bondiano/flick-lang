// A stand-in for an external user directory: answers after a delay that
// grows with the id, so the second of two parallel loads finishes first.
const names = new Map([
  ["u1", "  Ada Lovelace "],
  ["u2", "Grace Hopper"],
]);

const delays = new Map([["u1", 50], ["u2", 1]]);

export const findName = (id) =>
  new Promise((resolve) => setTimeout(() => resolve(names.get(id) ?? null), delays.get(id) ?? 0));
