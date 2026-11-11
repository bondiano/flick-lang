package flickrt

import (
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

// The built-in behavior of `--trace`.

// cliTraced traces a handler: under `--trace` the handler of command `name`
// runs as a span of that name, sending to OTEL_EXPORTER_OTLP_ENDPOINT as
// service OTEL_SERVICE_NAME. Sampled spans go to /v1/traces as they end,
// every 5 s; log records to /v1/logs through a logger added to the fiber's,
// every second; the metrics to /v1/metrics, every 10 s. What is left is
// sent as the command ends, each within 3 s. Log messages also reach the
// spans as the tracer logger's events.
func cliTraced(fb *Fiber, name string, run func(*Fiber) Value) Value {
	endpoint, service := os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT"), os.Getenv("OTEL_SERVICE_NAME")
	if endpoint == "" {
		endpoint = "http://localhost:4318"
	}
	if service == "" {
		service = name
	}
	resource := otlpResource(service)
	traces := newOtlpExporter(joinSegments(endpoint, "/v1/traces"), "OtlpTracer", 5*time.Second, 1000, false, func(spans []string) string {
		return `{"resourceSpans":[{"resource":` + resource + `,"scopeSpans":[{"scope":` + otlpScope +
			`,"spans":[` + strings.Join(spans, ",") + `]}]}]}`
	})
	logs := newOtlpExporter(joinSegments(endpoint, "/v1/logs"), "OtlpLogger", time.Second, 1000, false, func(records []string) string {
		return otlpLogs(resource, records)
	})
	traced := *fb
	clock, metrics := clockOf(fb), metricContextOf(&traced)
	start := clock.nanos()
	metered := newOtlpExporter(joinSegments(endpoint, "/v1/metrics"), "OtlpMetrics", 10*time.Second, 0, true, func([]string) string {
		return otlpMetrics(resource, service, start, clock.nanos(), snapshotOf(metrics))
	})
	keep := &tracer{ended: func(s *span) {
		if s.sampled {
			traces.push(otlpSpan(s))
		}
	}}
	logger := &logger{func(fb *Fiber, m *logEntry) Value {
		logs.push(otlpLogRecord(fb, m))
		return Unit
	}}
	traced.env = fb.env.with(tracerKey, keep).with(loggersKey, loggerSet(loggersOf(fb), []Value{logger}))
	out, cause := catch(func() Value {
		return TracerWithSpan(&traced, name, Rec("parent", None, "root", false), Eff(run))
	})
	var shutdowns sync.WaitGroup
	for _, e := range []*otlpExporter{traces, logs, metered} {
		shutdowns.Add(1)
		go func() {
			defer shutdowns.Done()
			e.shutdown()
		}()
	}
	shutdowns.Wait()
	if cause != nil {
		panic(&Failure{cause})
	}
	return out
}

var otlpKinds = map[string]string{"internal": "1", "server": "2", "client": "3", "producer": "4", "consumer": "5"}

// otlpValue is the OTLP attribute value of a Flick value.
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

// otlpSpan is the OTLP form of an ended span.
func otlpSpan(s *span) string {
	s.mu.Lock()
	defer s.mu.Unlock()
	attrs := append([]spanAttr{}, s.attrs...)
	events := make([]string, len(s.events))
	for i, e := range s.events {
		events[i] = `{"name":` + QuoteJSON(e.name) + `,"timeUnixNano":` + QuoteJSON(e.time.String()) +
			`,"attributes":` + otlpAttributes(e.attrs) + `,"droppedAttributesCount":0}`
	}
	status := `{"code":1}`
	if s.exit != nil {
		if !s.exit.has(causeFail) && !s.exit.has(causeDie) {
			status = `{"code":0}`
			attrs = append(attrs, spanAttr{"effect.fiber.interrupted", true})
		} else if errs := prettyErrors(s.exit); len(errs) > 0 {
			status = `{"code":2,"message":` + QuoteJSON(errs[0].message) + `}`
			for _, e := range errs {
				events = append(events, `{"name":"exception","timeUnixNano":`+QuoteJSON(s.end.String())+
					`,"droppedAttributesCount":0,"attributes":`+otlpAttributes([]spanAttr{
					{"exception.type", e.name}, {"exception.message", e.message}, {"exception.stacktrace", e.rendered()},
				})+"}")
			}
		}
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
