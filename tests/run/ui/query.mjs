// How many times each query of the query run test ran.
const runs = new Map();

export const run = (name) => {
  runs.set(name, (runs.get(name) ?? 0) + 1);
  return runs.get(name);
};
