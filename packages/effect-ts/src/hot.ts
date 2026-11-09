// Module replacement in development: what the UI layer gives the browser
// runner so a replaced program's states carry over. The runner needs no UI,
// so it reaches the UI's roots only through these hooks.

let states = (): Map<string, unknown> => new Map();
let carry = (_states: ReadonlyMap<string, unknown>): void => undefined;

/** Called by the UI layer: how to read the labelled states of its roots, and how the next root starts from them. */
export const hotHooks = (read: () => Map<string, unknown>, next: (states: ReadonlyMap<string, unknown>) => void): void => {
  states = read;
  carry = next;
};

export const hotStates = (): Map<string, unknown> => states();

export const hotCarryTo = (values: ReadonlyMap<string, unknown>): void => carry(values);
