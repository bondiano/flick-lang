// Relative Eff foreigns preserve their host Effect ABI on --rt.
// Compare the corresponding host owner only during raw execution; transporting
// the native token through the factory must not create an owner or register hooks.
import { hostOwnerOf } from "../../packages/effect-ts/dist/bridge_host.js";
import { makePolicy } from "./owner_foreign_policy.mjs";
export { inspect, markProvider } from "./owner_foreign_policy.mjs";
export const makeNonregular = (owner) => makePolicy("nonregular", owner, hostOwnerOf);
export const makeRow = (owner) => makePolicy("row", owner, hostOwnerOf);
export const makeProjected = (owner) => makePolicy("projected", owner, hostOwnerOf);
export const makeLongId = (owner) => makePolicy("longid", owner, hostOwnerOf);
export const makeChain = (owner) => makePolicy("chain", owner, hostOwnerOf);
export const makeHigher = (owner) => makePolicy("higher", owner, hostOwnerOf);
export const makeIdle = (owner) => makePolicy("idle", owner, hostOwnerOf);
