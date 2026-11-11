// Stack-safe pure loops on Flick's own runtime.
import type { Tagged } from "../core/data.ts";
import { tag } from "../tags.ts";

const CONTINUE = /*#__PURE__*/ tag("Loop.Step.Continue");
/** `Loop.Step s a`: the tags alone tell `Continue` from `Done`. */
export type Step<S, A> = (Tagged<string, S> & { readonly continue?: true }) | (Tagged<string, A> & { readonly continue?: false });
const isContinue = <S, A>(step: Step<S, A>): step is Tagged<string, S> & { readonly continue?: true } =>
  step._tag === CONTINUE;

export const loop = <S, A>(step: (state: S) => Step<S, A>, state: S): A => {
  let current = state;
  for (;;) {
    const next = step(current);
    if (!isContinue(next)) return next._0;
    current = next._0;
  }
};

export type Trampoline<A> =
  | { readonly done: true; readonly value: A }
  | { readonly done: false; readonly thunk: (unit: undefined) => Trampoline<A> };

export const trampolineDone = <A>(value: A): Trampoline<A> => ({ done: true, value });
export const trampolineSuspend = <A>(thunk: (unit: undefined) => Trampoline<A>): Trampoline<A> => ({ done: false, thunk });
export const trampolineRun = <A>(term: Trampoline<A>): A => {
  let current = term;
  while (!current.done) current = current.thunk(undefined);
  return current.value;
};
