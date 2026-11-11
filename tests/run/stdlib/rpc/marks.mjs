// Marks the handlers of the local run test leave once they finish, and
// the handlers that have started, which a client waits for before it gives up.
const marks = [];
export const mark = (name) => {
  marks.push(name);
  return undefined;
};
export const marked = () => marks.join(", ");

const begun = new Map();
const signal = (name) => {
  if (!begun.has(name)) begun.set(name, Promise.withResolvers());
  return begun.get(name);
};
export const start = (name) => {
  signal(name).resolve(undefined);
  return undefined;
};
export const started = (name) => signal(name).promise;
