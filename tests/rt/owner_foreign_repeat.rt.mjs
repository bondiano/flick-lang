// Keep the host Effect ABI; normalize identity only at actual raw execution.
import { hostOwnerOf } from "../../packages/effect-ts/dist/bridge_host.js";
import { makeRepeat as make } from "./owner_foreign_repeat.mjs";
export const makeRepeat = (owner) => make(owner, hostOwnerOf);
