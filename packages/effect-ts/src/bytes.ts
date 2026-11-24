// The packed bytes of Flick's own runtime, with the options and results its
// functions return made the adapter's: `==` compares them with what Flick's
// `Some`, `None`, `Ok` and `Err` build on Effect.
import type { Option as CoreOption, Result as CoreResult } from "@flick-lang/runtime/core/data";
import * as Core from "@flick-lang/runtime/core/bytes";
import type { Option, Result } from "effect";

import { err, none, ok, some } from "./data.ts";

export * from "@flick-lang/runtime/core/bytes";

type Bytes = Parameters<typeof Core.bytesLength>[0];

const option = <A>(o: CoreOption<A>): Option.Option<A> => (o._tag === "Some" ? some(o.value) : none);
const result = <A, E>(r: CoreResult<A, E>): Result.Result<A, E> => (r._tag === "Success" ? ok(r.success) : err(r.failure));

export const bytesGet = (index: number, value: Bytes): Option.Option<number> => option(Core.bytesGet(index, value));
export const bytesSlice = (start: number, end: number, value: Bytes): Option.Option<Bytes> =>
  option(Core.bytesSlice(start, end, value));
export const bytesFromList = (list: ReadonlyArray<number>): Result.Result<Bytes, readonly [number, number]> =>
  result(Core.bytesFromList(list));
export const bytesDecodeUtf8 = (value: Bytes): Option.Option<string> => option(Core.bytesDecodeUtf8(value));
export const bytesDecodeBase64Url = (text: string): Option.Option<Bytes> => option(Core.bytesDecodeBase64Url(text));
