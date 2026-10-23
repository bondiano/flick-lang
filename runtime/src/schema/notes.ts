// The annotations under `flick/…` the runtime puts on Effect Schema nodes,
// for the formatter of issues, the exporter, the generators and the
// detection of runtime decoding. They extend Effect's annotations, so
// writing and reading them is typed.
import type { Schema } from "effect";

import type { Rng } from "../stdlib/testing.ts";

// oxlint-disable no-shadow -- Effect's annotations are extended by merging into its own namespace
declare module "effect/Schema" {
  namespace Annotations {
    interface Annotations {
      readonly expected?: string | undefined;
      readonly message?: string | undefined;
      readonly messageMissingKey?: string | undefined;
      /** Names of the variants of a union, in order. */
      readonly "flick/tags"?: ReadonlyArray<string>;
      /** A union whose members exclude each other: `oneOf` in JSON Schema. */
      readonly "flick/oneOf"?: boolean;
      /** The constructor name of a variant's struct. */
      readonly "flick/variant"?: string;
      /** The name of a recursive type, for `$defs`. */
      readonly "flick/ref"?: string;
      /** Why a schema has no wire form. */
      readonly "flick/refuse"?: string;
      /** A check JSON Schema can state, by name. */
      readonly "flick/known"?: string | null;
      /** The refined type a check belongs to. */
      readonly "flick/refined"?: string;
      /** A check every JSON value passes: failing it means there is no wire form. */
      readonly "flick/wire"?: boolean;
      readonly "flick/pattern"?: string;
      readonly "flick/transform"?: "pure" | "result" | "effect";
      /** A field whose default needs services. */
      readonly "flick/effect"?: boolean;
      /** The record a `codec` block describes, and its wire names by label. */
      readonly "flick/block"?: string;
      readonly "flick/wires"?: Readonly<Record<string, string>>;
      readonly "flick/ignoreExtra"?: boolean;
      readonly "flick/generator"?: (rng: Rng) => unknown;
    }
  }
}
// oxlint-enable no-shadow

export type Notes = Schema.Annotations.Annotations;

const none: Notes = {};

/** The annotations of an AST node, a check or an issue. */
export const notesOf = (node: { readonly annotations?: Notes | undefined } | undefined): Notes =>
  node?.annotations ?? none;
