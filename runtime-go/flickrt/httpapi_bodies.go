package flickrt

import "strings"

// The bodies of HttpApi endpoints besides JSON, text and URL-encoded
// forms, as the server writes them: bytes, streams of bytes, Server-Sent
// Events, and the response headers and cookie a handler answers with its
// value.

// apiStrings is the fields of an encoded record as strings; null and
// absent fields are left out, other values written as JSON.
func apiStrings(encoded Value) []Value {
	r, ok := encoded.(*Record)
	if !ok {
		return []Value{}
	}
	out := make([]Value, 0, len(r.keys))
	for i, k := range r.keys {
		switch v := r.vals[i].(type) {
		case string:
			out = append(out, []Value{k, v})
		default:
			if v != Unit && v != JsonNull {
				out = append(out, []Value{k, JsonStringify(v)})
			}
		}
	}
	return out
}

// sseEvent is an event of the lines of its fields.
func sseEvent(fields ...string) string {
	var b strings.Builder
	first := true
	for i := 0; i < len(fields); i += 2 {
		for _, line := range strings.Split(fields[i+1], "\n") {
			if !first {
				b.WriteByte('\n')
			}
			first = false
			b.WriteString(fields[i] + ": " + line)
		}
	}
	b.WriteString("\n\n")
	return b.String()
}

func sseResponse(status int32, text *Stream) Value {
	return bodyResponse(noOptions, status, nil, *streamBody(StreamEncodeText(text), "text/event-stream", -1))
}

// apiOtherSuccess is the response of a success of bytes, a stream or
// events; nil for another encoding.
func apiOtherSuccess(e *apiEndpoint, value Value) Value {
	switch e.successEncoding {
	case encBytes:
		return bodyResponse(noOptions, e.status, nil, *bytesBody(byteString(value), "application/octet-stream"))
	case encStream:
		return bodyResponse(noOptions, e.status, nil, *streamBody(value, "application/octet-stream", -1))
	case encEvents:
		return sseResponse(e.status, mapElements(streamOf(value), func() func(*Fiber, Value) Value {
			return func(_ *Fiber, x Value) Value {
				r := record(x)
				if id, ok := some(r.Get("id")); ok {
					return sseEvent("id", id.(string), "event", r.Get("event").(string), "data", r.Get("data").(string))
				}
				return sseEvent("event", r.Get("event").(string), "data", r.Get("data").(string))
			}
		}))
	case encSSE:
		c := e.success
		if c == nil {
			return nil
		}
		return sseResponse(e.status, mapElements(streamOf(value), func() func(*Fiber, Value) Value {
			return func(fb *Fiber, x Value) Value {
				encoded, issues := c.encode(fb, x)
				if issues != nil {
					return Fail(schemaErrorOf(issues))
				}
				return sseEvent("data", JsonStringify(encoded).(string))
			}
		}))
	}
	return nil
}

// apiUnwrapExtras is the handler's value without its extras, and what they
// add to the response: the encoded headers, and the cookie, Secure and
// HttpOnly.
func apiUnwrapExtras(fb *Fiber, extras []apiExtra, v Value) (Value, func(Value) Value) {
	inner := v
	var steps []func(Value) Value
	for i := len(extras) - 1; i >= 0; i-- {
		pair := List(inner)
		inner = pair[0]
		added := pair[1]
		if c := extras[i].headers; c != nil {
			encoded, issues := c.encode(fb, added)
			if issues != nil {
				Fail(schemaErrorOf(issues))
			}
			headers := apiStrings(encoded)
			steps = append(steps, func(r Value) Value { return ResponseSetHeaders(headers, r) })
			continue
		}
		if cookie, ok := some(added); ok {
			key := extras[i].cookie
			steps = append(steps, func(r Value) Value {
				bits := uint16(optHttpOnly | optSecure)
				o := cookieOptions{keys: bits, set: bits, httpOnly: true, secure: true}
				c, reason := makeCookie(key, RedactedReveal(cookie).(string), &o)
				if reason != "" {
					failCookie(reason)
				}
				s := responseOf(r)
				return s.changed(s.headers, jarOf(jarOf(s.cookies).set(key, c)), s.body, false)
			})
		}
	}
	return inner, func(r Value) Value {
		for _, step := range steps {
			r = step(r)
		}
		return r
	}
}
