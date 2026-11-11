// URLs of a type that derives `Url`, on Effect: the runtime's codec over
// Effect's schemas and `Option`s.
import { Option } from "effect";
import { type UrlCodec as Codec, routes } from "@flick-lang/runtime/core/route";

import { type AnySchema, schemaValidate } from "./schema/index.ts";

/** `Url a`: what `Route.print` and `Route.parse` read. */
export type UrlCodec = Codec<AnySchema>;

export const {
  coerce,
  routeCodec,
  routeNested,
  routeNestedOf,
  routeParse,
  routePath,
  routePrint,
  routeQuery,
  routeSegment,
  routeWords,
  routeWordsOf,
} = /*#__PURE__*/ routes<AnySchema, Option.Option<unknown>>({
  none: Option.none,
  some: Option.some,
  validate: schemaValidate,
});
