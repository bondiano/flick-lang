// Nonretaining instrumentation: observe the ambient default owner, not an arena.
import { currentCtx } from "../../runtime/dist/rt/fiber.js";

export const sameOwner = (owner) => currentCtx().owner === owner;
