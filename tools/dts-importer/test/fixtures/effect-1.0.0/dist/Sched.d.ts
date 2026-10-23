/** A schedule. */
export interface Sched<Out> { readonly out: Out }
export declare const spaced: (millis: number) => Sched<number>;
export declare const forever: Sched<number>;
export declare const removed: Sched<string>;
