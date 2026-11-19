// Arena storage belongs to an opaque owner and retires after its finalizers.
// Both runtime adapters use the same lifecycle; allocation stays independently
// removable from programs that never make an arena.
import { Arena } from "../core/Arena.ts";

const storage = /*#__PURE__*/ new WeakMap<object, Array<Arena> | null>();

export const arenasClosed = (owner: object): boolean => storage.get(owner) === null;

export const closeArenas = (owner: object): void => {
  const arenas = storage.get(owner);
  storage.set(owner, null);
  arenas?.forEach((arena) => arena.close());
};

export const ownedArena = (owner: object): Arena => {
  let arenas = storage.get(owner);
  if (arenas === null) throw new Error("arena owner is closed");
  if (arenas === undefined) { arenas = []; storage.set(owner, arenas); }
  const arena = new Arena();
  arenas.push(arena);
  return arena;
};
