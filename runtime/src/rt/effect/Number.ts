// Numeric reducers beside the Effect-free Float operations.
export * from "../../core/Number.ts";
export const ReducerSum = { initial: 0, combine: (a: number, b: number): number => a + b };
export const ReducerMultiply = { initial: 1, combine: (a: number, b: number): number => a * b };
export const ReducerMax = { initial: Number.NEGATIVE_INFINITY, combine: (a: number, b: number): number => Math.max(a, b) };
export const ReducerMin = { initial: Number.POSITIVE_INFINITY, combine: (a: number, b: number): number => Math.min(a, b) };
