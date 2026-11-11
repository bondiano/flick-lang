// URLs of a type that derives `Url`, on Flick's own runtime: the codec of
// `core/route.ts` over this runtime's schemas and plain `Option`s.
import { type Option, none, some } from "../core/data.ts";
import { type UrlCodec as Codec, routes } from "../core/route.ts";
import { type Witness, schemaValidate } from "./schema.ts";

/** `Url a`: what `Route.print` and `Route.parse` read. */
export type UrlCodec = Codec<Witness>;

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
} = /*#__PURE__*/ routes<Witness, Option<unknown>>({ none: () => none, some, validate: schemaValidate });
