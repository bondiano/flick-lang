package flickrt

import (
	"bytes"
	"math"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
)

// The built-in behavior of `--trace`.

// cliTraced is run.ts's `traced`: under `--trace` the handler of command
// `name` runs as a span of that name with a tracer that keeps every
// sampled span as it ends, sent over OTLP/HTTP JSON to
// OTEL_EXPORTER_OTLP_ENDPOINT as service OTEL_SERVICE_NAME when the
// command ends. Log messages reach the spans as the tracer logger's events.
// shortcut: no log records or metrics are exported, and a failed span has no `exception` events; port Effect's Otlp logger, metrics and Cause.prettyErrors when a program reads them
func cliTraced(fb *Fiber, name string, run func(*Fiber) Value) Value {
	var mu sync.Mutex
	var spans []*span
	keep := &tracer{ended: func(s *span) {
		if s.sampled {
			mu.Lock()
			spans = append(spans, s)
			mu.Unlock()
		}
	}}
	traced := *fb
	traced.env = fb.env.with(tracerKey, keep)
	out, cause := catch(func() Value {
		return TracerWithSpan(&traced, name, Rec("parent", None, "root", false), Eff(run))
	})
	endpoint, service := os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT"), os.Getenv("OTEL_SERVICE_NAME")
	if endpoint == "" {
		endpoint = "http://localhost:4318"
	}
	if service == "" {
		service = name
	}
	mu.Lock()
	body := cliOtlpSpans(service, spans)
	mu.Unlock()
	if response, err := http.Post(endpoint+"/v1/traces", "application/json", bytes.NewReader([]byte(body))); err == nil {
		response.Body.Close()
	}
	if cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// cliOtlpSpans is an ExportTraceServiceRequest of the spans, in OTLP's JSON.
func cliOtlpSpans(service string, spans []*span) string {
	encoded := make([]string, len(spans))
	for i, s := range spans {
		encoded[i] = otlpSpan(s)
	}
	return `{"resourceSpans":[{"resource":{"attributes":[{"key":"telemetry.sdk.name","value":{"stringValue":"effect"}},` +
		`{"key":"telemetry.sdk.language","value":{"stringValue":"go"}},{"key":"telemetry.sdk.version","value":{"stringValue":"4.0.5"}},` +
		`{"key":"service.name","value":{"stringValue":` + QuoteJSON(service) +
		`}}],"droppedAttributesCount":0},"scopeSpans":[{"scope":{"name":"effect","version":"4.0.5"},"spans":[` +
		strings.Join(encoded, ",") + `]}]}]}`
}

var otlpKinds = map[string]string{"internal": "1", "server": "2", "client": "3", "producer": "4", "consumer": "5"}

// otlpValue is Effect's unknownToAttributeValue.
func otlpValue(v Value) string {
	switch x := v.(type) {
	case string:
		return `{"stringValue":` + QuoteJSON(x) + `}`
	case bool:
		return `{"boolValue":` + strconv.FormatBool(x) + `}`
	case int32:
		return `{"intValue":` + strconv.Itoa(int(x)) + `}`
	case float64:
		if x == math.Trunc(x) && !math.IsInf(x, 0) {
			return `{"intValue":` + jsString(x) + `}`
		}
		return `{"doubleValue":` + jsonNumber(x) + `}`
	case []Value:
		values := make([]string, len(x))
		for i, e := range x {
			values[i] = otlpValue(e)
		}
		return `{"arrayValue":{"values":[` + strings.Join(values, ",") + `]}}`
	}
	return `{"stringValue":` + QuoteJSON(attrText(v)) + `}`
}

func otlpAttributes(attrs []spanAttr) string {
	parts := make([]string, len(attrs))
	for i, a := range attrs {
		parts[i] = `{"key":` + QuoteJSON(a.key) + `,"value":` + otlpValue(a.value) + `}`
	}
	return "[" + strings.Join(parts, ",") + "]"
}

func otlpTraceId(id string) string {
	if n := 32 - len(id); n > 0 {
		return strings.Repeat("0", n) + id
	}
	return id
}

// otlpSpan is Effect's makeOtlpSpan of an ended span.
func otlpSpan(s *span) string {
	s.mu.Lock()
	defer s.mu.Unlock()
	attrs := append([]spanAttr{}, s.attrs...)
	status := `{"code":1}`
	if s.exit != nil {
		if !s.exit.has(causeFail) && !s.exit.has(causeDie) {
			status = `{"code":0}`
			attrs = append(attrs, spanAttr{"effect.fiber.interrupted", true})
		} else {
			message, _, _ := strings.Cut(pretty(s.exit), "\n")
			status = `{"code":2,"message":` + QuoteJSON(message) + `}`
		}
	}
	events := make([]string, len(s.events))
	for i, e := range s.events {
		events[i] = `{"name":` + QuoteJSON(e.name) + `,"timeUnixNano":` + QuoteJSON(e.time.String()) +
			`,"attributes":` + otlpAttributes(e.attrs) + `,"droppedAttributesCount":0}`
	}
	links := make([]string, len(s.links))
	for i, l := range s.links {
		link := record(l)
		var attributes []spanAttr
		for _, kv := range List(link.Get("attributes")) {
			attributes = append(attributes, spanAttr{At(kv, 0).(string), At(kv, 1)})
		}
		linked := spanOf(link.Get("span"))
		links[i] = `{"traceId":` + QuoteJSON(otlpTraceId(linked.traceId)) + `,"spanId":` + QuoteJSON(linked.spanId) +
			`,"attributes":` + otlpAttributes(attributes) + `,"droppedAttributesCount":0}`
	}
	parent := ""
	if p, ok := some(s.parent); ok {
		parent = `"parentSpanId":` + QuoteJSON(spanOf(p).spanId) + `,`
	}
	name, _ := some(s.name)
	kind := otlpKinds[s.kind]
	if kind == "" {
		kind = "1"
	}
	return `{"traceId":` + QuoteJSON(otlpTraceId(s.traceId)) + `,"spanId":` + QuoteJSON(s.spanId) + `,` + parent +
		`"name":` + QuoteJSON(name.(string)) + `,"kind":` + kind + `,"startTimeUnixNano":` +
		QuoteJSON(s.start.String()) + `,"endTimeUnixNano":` + QuoteJSON(s.end.String()) + `,"attributes":` +
		otlpAttributes(attrs) + `,"droppedAttributesCount":0,"events":[` + strings.Join(events, ",") +
		`],"droppedEventsCount":0,"status":` + status + `,"links":[` + strings.Join(links, ",") + `],"droppedLinksCount":0}`
}
