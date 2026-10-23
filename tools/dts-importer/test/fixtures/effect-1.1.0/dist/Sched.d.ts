/** A schedule, documented differently: comments do not change fingerprints. */
export interface Sched<Out> { readonly out: Out }
export declare const spaced: (millis: number) => Sched<number>;
export declare const forever: (limit: number) => Sched<number>;
export declare const added: Sched<boolean>;
