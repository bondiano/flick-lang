// TypeScript forms of Flick values compiled for the owned runtime. These are
// type-only: importing a generated .d.ts never loads the Effect adapter.
import type { Option as DataOption, Result as DataResult } from "../core/data.ts";
import type { Ui as RuntimeUi } from "../ui/react/nodes.ts";
import type { Layer as RuntimeLayer } from "./layer.ts";
import type { Codec as RuntimeCodec } from "./schema.ts";

declare const errors: unique symbol;
declare const services: unique symbol;
declare const schema: unique symbol;
declare const stream: unique symbol;
/** A stage may finish immediately, yield to the driver, or be held as a task. */
export type EffectValue<A, E, R> =
  | A
  | (Generator<unknown, A, unknown> & { readonly [errors]?: E; readonly [services]?: R })
  | ((() => A | Generator<unknown, A, unknown>) & { readonly [errors]?: E; readonly [services]?: R });

export type OptionValue<A> = DataOption<A>;
export type ResultValue<A, E = never> = DataResult<A, E>;
export type LayerValue<Out, E, In> = RuntimeLayer & { readonly [services]?: readonly [Out, E, In] };
export type SchemaValue<A, I = A, R = never, E = never> = RuntimeCodec & {
  readonly [schema]?: (input: I, services: R, error: E) => A;
};
/** Stream values are opaque until a bridge or a native implementation runs them. */
export interface StreamValue<A, E, R> {
  readonly [stream]: readonly [A, E, R];
}
export type Ui = RuntimeUi;

/** An interned native service key with its service and implementation types. */
export interface ServiceValue<Self, Impl> {
  readonly key: string;
  readonly [services]?: readonly [Self, Impl];
}
