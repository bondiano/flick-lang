export { jsonCodec as Json } from "../schema.ts";
export { schemaList as flickChunkJson } from "../schema.ts";
import { type Codec } from "../schema.ts";
export const toDifferJsonPatch = <A>(codec: Codec<A>): { readonly codec: Codec<A> } => ({ codec });
export const flickToType = <A>(codec: A): A => codec;
