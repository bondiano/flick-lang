import { Effect, scopeCurrent } from "@flick-lang/effect-ts";

const states = new Map();
const providers = new Map();
export const makePolicy = (label, owner, normalize = (value) => value) => {
  if (states.has(label)) throw new Error(`${label}: factory called twice`);
  const state = { steps: 0 };
  states.set(label, state);
  const observe = (current) => {
    const provider = label === "row" && state.steps === 0;
    const expected = normalize(owner);
    if (current !== (provider ? providers.get(label) : expected) || (provider && current === expected)) throw new Error(`${label}: wrong raw owner at ${state.steps}`);
    state.steps++;
    return label === "idle" ? undefined : work;
  };
  const work = Effect.map(scopeCurrent(), observe);
  return work;
};

// Reading this before the first await checks construction is cold; after the
// last await it also detects extra evaluation and discarded raw factory calls.
export const inspect = (label, expected) => {
  const actual = states.get(label)?.steps ?? 0;
  if (BigInt(actual) !== expected) throw new Error(`${label}: expected ${expected} steps, got ${actual}`);
  return `${label}:${actual}`;
};
export const makeNonregular = (owner) => makePolicy("nonregular", owner);
export const makeRow = (owner) => makePolicy("row", owner);
export const makeProjected = (owner) => makePolicy("projected", owner);
export const makeLongId = (owner) => makePolicy("longid", owner);
export const makeChain = (owner) => makePolicy("chain", owner);
export const makeHigher = (owner) => makePolicy("higher", owner);
export const makeIdle = (owner) => makePolicy("idle", owner);

// This unscoped raw task observes the actual Layer provider independently of
// the descriptor under test. A widened row must not activate outer here.
export const markProvider = (label) => Effect.map(scopeCurrent(), (owner) => { providers.set(label, owner); });
