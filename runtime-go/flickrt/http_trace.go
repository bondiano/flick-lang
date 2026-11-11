package flickrt

import "strings"

// Http.TraceContext: the B3 and W3C headers of a span, and the external
// span of a request's headers.

func TraceContextToHeaders(s Value) Value {
	sp := spanOf(s)
	sampled, flags := "0", "00"
	if sp.sampled {
		sampled, flags = "1", "01"
	}
	b3 := sp.traceId + "-" + sp.spanId + "-" + sampled
	if p, ok := some(sp.parent); ok && !sp.external {
		b3 += "-" + spanOf(p).spanId
	}
	traceId := sp.traceId
	if n := 32 - len(traceId); n > 0 {
		traceId = strings.Repeat("0", n) + traceId
	}
	h := newRecord(2)
	h.put("b3", b3)
	h.put("traceparent", "00-"+traceId+"-"+sp.spanId+"-"+flags)
	return h
}

func isHex(s string, sizes ...int) bool {
	for _, n := range sizes {
		if len(s) == n && strings.Trim(strings.ToLower(s), "0123456789abcdef") == "" {
			return true
		}
	}
	return false
}

func externalSpan(traceId, spanId string, sampled bool) Value {
	return Some(&span{name: None, spanId: spanId, traceId: traceId, sampled: sampled, parent: None, external: true})
}

func headerText(h Value, name string) string {
	s, _ := recordField(headersOf(h), name).(string)
	return s
}

// TraceContextW3c is the span of a version 00 `traceparent` header.
func TraceContextW3c(h Value) Value {
	parts := strings.Split(headerText(h, "traceparent"), "-")
	if len(parts) != 4 || parts[0] != "00" || !isHex(parts[1], 32) || !isHex(parts[2], 16) ||
		parts[1] == strings.Repeat("0", 32) || parts[2] == strings.Repeat("0", 16) {
		return None
	}
	// `parseInt(flags, 16) & 1`: the parity of the leading hex digits, of
	// a number exact below 2^53
	digits := len(parts[3]) - len(strings.TrimLeft(strings.ToLower(parts[3]), "0123456789abcdef"))
	sampled := digits > 0 && digits <= 13 && strings.IndexByte("13579bdfBDF", parts[3][digits-1]) >= 0
	return externalSpan(parts[1], parts[2], sampled)
}

func TraceContextB3(h Value) Value {
	v, ok := recordField(headersOf(h), "b3").(string)
	if !ok {
		return None
	}
	parts := strings.Split(v, "-")
	if len(parts) < 2 || !isHex(parts[0], 16, 32) || !isHex(parts[1], 16) {
		return None
	}
	sampled := len(parts) < 3 || parts[2] == "" || parts[2] == "1" || parts[2] == "d"
	return externalSpan(parts[0], parts[1], sampled)
}

func TraceContextXb3(h Value) Value {
	traceId, spanId := headerText(h, "x-b3-traceid"), headerText(h, "x-b3-spanid")
	if !isHex(traceId, 16, 32) || !isHex(spanId, 16) {
		return None
	}
	s := headerText(h, "x-b3-sampled")
	sampled := headerText(h, "x-b3-flags") == "1" || s == "" || s == "1" || s == "true"
	return externalSpan(traceId, spanId, sampled)
}

// TraceContextFromHeaders is the span of the first format the headers
// carry: W3C, B3, then X-B3.
func TraceContextFromHeaders(h Value) Value {
	if s := TraceContextW3c(h); isSome(s) {
		return s
	}
	if s := TraceContextB3(h); isSome(s) {
		return s
	}
	return TraceContextXb3(h)
}
