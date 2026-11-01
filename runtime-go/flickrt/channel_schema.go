package flickrt

import "strings"

// Effect's `ChannelSchema` (ChannelSchema.ts): each chunk decoded or
// encoded with the codec as Effect's `NonEmptyArray` of it, an issue's
// path led by the element's index, failing with the program's
// `SchemaError`. Decoding is strict, as `onExcessProperty: "error"`, and
// an issue does not name the input, as without `reportInput`.

// channelCodec runs `code` of the chunk's codec on each chunk of the
// upstream; its errors and end pass through.
func channelCodec(c Value, code func(l *listCodec, fb *Fiber, chunk Value) (Value, []issue)) *Channel {
	l := &listCodec{codecOf(c)}
	return fromTransform(func(_ *Fiber, upstream Pull, _ *Owner) Pull {
		return func(fb *Fiber) Value {
			out, issues := code(l, fb, upstream(fb))
			if issues != nil {
				return Fail(schemaErrorOf(unreported(issues)))
			}
			return out
		}
	})
}

// unreported is the issues without the inputs they name: a number with no
// JSON form fails the wire's check, whose text Effect falls back to.
func unreported(issues []issue) []issue {
	out := make([]issue, len(issues))
	for i, is := range issues {
		if at := strings.Index(is.message, ", got "); at >= 0 && strings.HasPrefix(is.message, "expected ") {
			is.message = is.message[:at]
		} else if strings.HasSuffix(is.message, " has no JSON form") {
			is.message = "expected a finite number"
		}
		out[i] = is
	}
	return out
}

func ChannelSchemaDecode(c Value) Value { return channelCodec(c, (*listCodec).decode) }
func ChannelSchemaEncode(c Value) Value { return channelCodec(c, (*listCodec).encode) }

// ChannelSchemaDuplex is the channel between the input encoded and the
// output decoded.
func ChannelSchemaDuplex(options, self Value) Value {
	o := record(options)
	encoded := channelPipeTo(channelCodec(o.Get("inputSchema"), (*listCodec).encode), channelOf(self))
	return channelPipeTo(encoded, channelCodec(o.Get("outputSchema"), (*listCodec).decode))
}
