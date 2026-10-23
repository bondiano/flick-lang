// Effect's `ChannelSchema` for the bindings of the standard library: the
// adapters map Effect's `SchemaError` to the program's `SchemaError`.
import { Channel, ChannelSchema, Schema } from "effect";

import type { AnySchema } from "../schema/index.ts";
import { toSchemaError } from "../schema/issues.ts";

export * from "effect/ChannelSchema";

type NonEmpty<A> = readonly [A, ...Array<A>];
type Chunks = Channel.Channel<NonEmpty<unknown>, unknown, unknown, NonEmpty<unknown>, unknown, unknown, unknown>;

const OPTIONS = { onExcessProperty: "error" } as const;

const mapped =
  (encoding: boolean) =>
  (error: unknown): unknown =>
    error instanceof Schema.SchemaError ? toSchemaError(encoding)(error) : error;

/** `ChannelSchema.decode`. */
export const flickDecode = (codec: AnySchema): Chunks =>
  Channel.mapError(ChannelSchema.decode(codec, OPTIONS)(), mapped(false));

/** `ChannelSchema.encode`. */
export const flickEncode = (codec: AnySchema): Chunks => Channel.mapError(ChannelSchema.encode(codec)(), mapped(true));

/**
 * `ChannelSchema.duplex`: Effect's composition of encode, the channel and
 * decode, with the adapted stages so that the channel already sees the
 * program's `SchemaError` among its input errors.
 */
export const flickDuplex =
  (options: { readonly inputSchema: AnySchema; readonly outputSchema: AnySchema }) =>
  (self: Chunks): Chunks =>
    flickEncode(options.inputSchema).pipe(Channel.pipeTo(self), Channel.pipeTo(flickDecode(options.outputSchema)));
