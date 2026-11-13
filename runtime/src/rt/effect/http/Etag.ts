// `Http.Etag`: the `Generator` service of entity tags, from a file's size and
// modification time as Effect's generators write them.
import type { Option } from "../../../core/data.ts";
import { tag } from "../../../tags.ts";
import { type Layer, layerSucceed } from "../../layer.ts";

const WEAK = /*#__PURE__*/ tag("Http.Etag.Etag.Weak");
const STRONG = /*#__PURE__*/ tag("Http.Etag.Etag.Strong");

/** What the generators read of a `FileSystem.Info`. */
interface Info {
  readonly size: bigint;
  readonly mtime: Option<number>;
}

// the time goes through a Date as Effect's does, which drops a fraction of a millisecond
export const value = (info: Info): string =>
  `${info.size.toString(16)}-${info.mtime._tag === "Some" ? new Date(info.mtime.value).getTime().toString(16) : "0"}`;

const generator = (etag: string) => (service: object): Layer =>
  layerSucceed(service, { from_file_info: (info: Info) => ({ _tag: etag, _0: value(info) }) });

/** `Http.Etag.layer`. */
export const flickLayer = /*#__PURE__*/ generator(STRONG);

/** `Http.Etag.layer_weak`. */
export const flickLayerWeak = /*#__PURE__*/ generator(WEAK);
