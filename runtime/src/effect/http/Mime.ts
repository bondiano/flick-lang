// Effect's `Mime` for the bindings of the standard library.
import { Option } from "effect";
import * as Mime from "effect/http/Mime";

export * from "effect/http/Mime";

/** `Http.Mime.get_all_extensions`: the set as a sorted list. */
export const flickGetAllExtensions = (type: string): Option.Option<ReadonlyArray<string>> =>
  Option.map(Mime.getAllExtensions(type), (set) => [...set].toSorted());
