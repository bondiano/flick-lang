import { Effect } from "@flick-lang/runtime";

export let factoryCalls = 0;
export let promisesMade = 0;

const say = (text) => process.stdout.write(`${text}\n`);

/** A factory of an Effect: counts its calls. */
export const counted = (n) => {
  factoryCalls++;
  say(`factory called (${factoryCalls})`);
  return Effect.succeed(n * 2);
};

/** Makes a promise: counts them. */
export const later = (n) => {
  promisesMade++;
  say(`promise made (${promisesMade})`);
  return new Promise((resolve) => setTimeout(() => resolve(n + 1), 1));
};

export const fail = (message) => {
  throw new Error(message);
};

export const reject = (message) => Promise.reject(new Error(message));

export const log = (text) => {
  say(`log ${text}`);
  return 42; // a void binding normalizes this to ()
};

/** `readText(path, signal)`: rejects for a missing file. */
export const readText = (path, signal) => {
  say(`readText got a signal: ${signal instanceof AbortSignal}`);
  return path === "missing" ? Promise.reject(new Error(`ENOENT: ${path}`)) : Promise.resolve(`text of ${path}`);
};

export const fetchUser = (id) => Effect.succeed({ id, name: `user ${id}` });

export const lookup = (key) => (key === "known" ? "found" : null);

export const greet = (name, greeting) => `${greeting ?? "hello"}, ${name}`;

export const sum = (...xs) => xs.reduce((a, b) => a + b, 0);

/** Calls a two-argument callback as JS does. */
export const combine = (f, a, b) => f(a, b);
export const combineThree = (f, a, b, c) => f(a, b, c);

export const sameCallback = (left, right) => left === right;

/** Runs an Effect callback twice inside the caller's fiber. */
export const twice = (f) => Effect.zipWith(f(1), f(2), (a, b) => a + b);

/** Awaits a Promise callback. */
export const awaitBoth = async (f) => (await f(1)) + (await f(2));

export const emitter = () => {
  const listeners = new Set();
  return {
    on: (fn) => {
      listeners.add(fn);
      say(`listening (${listeners.size})`);
    },
    off: (fn) => {
      say(`removed the registered function: ${listeners.delete(fn)}`);
    },
    emit: (event) => {
      for (const fn of listeners) fn(event);
      say(`emitted ${event} to ${listeners.size}`);
    },
    leaked: () => [...listeners],
  };
};

export const on = (e, fn) => e.on(fn);
export const off = (e, fn) => e.off(fn);
export const emit = (e, event) => e.emit(event);

export const getter = () => ({
  get secret() {
    throw new Error("the getter ran");
  },
});
export const withFunction = () => ({ f: () => 1 });
export const cyclic = () => {
  const o = { name: "loop" };
  o.self = o;
  return o;
};
export const shared = () => {
  const leaf = { n: 1 };
  return { a: leaf, b: leaf, list: [1, 2, "three"] };
};
export const trap = () =>
  new Proxy({}, {
    ownKeys: () => {
      throw new Error("trap");
    },
  });
export const deep = (n) => {
  let v = {};
  for (let i = 0; i < n; i++) v = { v };
  return v;
};

/** Sorts its argument in place, as many JS functions do. */
export const sortInPlace = (xs) => xs.sort((a, b) => a - b);

const keep = [1, 2];
/** Returns an array it keeps; `spoil` changes it later. */
export const kept = () => keep;
export const spoil = () => {
  keep.push(99);
};

/** Edits the record it is given. */
export const rename = (user) => {
  user.name = "mallory";
  user.tags.push("b");
};

/** Changes the array a callback returns. */
export const pushInto = (f) => {
  const xs = f();
  xs.push(9);
  return xs.length;
};
