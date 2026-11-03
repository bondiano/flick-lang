package flickrt

import (
	"math"
	"math/big"
	"os"
	"strconv"
	"strings"
)

// The log records and metrics `--trace` sends, in OTLP's JSON as Effect's
// OtlpLogger and OtlpMetrics (cumulative) write them.

// otlpResource is Effect's OtlpResource.fromConfig of the service, its
// version OTEL_SERVICE_VERSION.
//
// shortcut: OTEL_RESOURCE_ATTRIBUTES is not read; port Effect's Config.Record of it when a program sets it
func otlpResource(service string) string {
	version := ""
	if v := os.Getenv("OTEL_SERVICE_VERSION"); v != "" {
		version = `,{"key":"service.version","value":{"stringValue":` + QuoteJSON(v) + `}}`
	}
	return `{"attributes":[{"key":"telemetry.sdk.name","value":{"stringValue":"effect"}},` +
		`{"key":"telemetry.sdk.language","value":{"stringValue":"go"}},{"key":"telemetry.sdk.version","value":{"stringValue":"4.1.2"}},` +
		`{"key":"service.name","value":{"stringValue":` + QuoteJSON(service) + `}}` + version + `],"droppedAttributesCount":0}`
}

const otlpScope = `{"name":"effect","version":"4.1.2"}`

// withAttr is the attributes with `key` set: a key they have keeps its
// place, as in a JS object.
func withAttr(attrs []spanAttr, key string, value Value) []spanAttr {
	for i := range attrs {
		if attrs[i].key == key {
			attrs[i].value = value
			return attrs
		}
	}
	return append(attrs, spanAttr{key, value})
}

var otlpSeverities = map[string]int{"Trace": 1, "Debug": 5, "Info": 9, "Warn": 13, "Error": 17, "Fatal": 21}

// otlpLogRecord is Effect's makeLogRecord of a message the fiber logs.
func otlpLogRecord(fb *Fiber, m *logEntry) string {
	a := logAnnotationsOf(fb)
	attrs := make([]spanAttr, 0, len(a.keys)+4)
	for i, k := range a.keys {
		attrs = append(attrs, spanAttr{k, a.vals[i]})
	}
	attrs = withAttr(attrs, "effect.fiberId", float64(fb.state.id))
	// the outermost of the log spans of one label wins
	for _, s := range logSpansOf(fb) {
		attrs = withAttr(attrs, "effect.log_span."+s.label, m.date-s.start)
	}
	if m.cause != nil {
		if errs := prettyErrors(m.cause); len(errs) > 0 {
			stacks := make([]string, len(errs))
			for i, e := range errs {
				stacks[i] = e.rendered()
			}
			attrs = withAttr(attrs, "exception.type", errs[0].name)
			attrs = withAttr(attrs, "exception.message", errs[0].message)
			attrs = withAttr(attrs, "exception.stacktrace", strings.Join(stacks, "\n"))
		}
	}
	body := Value(m.message)
	if len(m.message) == 1 {
		body = m.message[0]
	}
	event := new(big.Int).Mul(big.NewInt(int64(m.date)), big.NewInt(1_000_000))
	out := `{"severityNumber":` + strconv.Itoa(otlpSeverities[m.level]) + `,"severityText":` + QuoteJSON(m.level) +
		`,"timeUnixNano":` + QuoteJSON(event.String()) + `,"observedTimeUnixNano":` + QuoteJSON(clockOf(fb).nanos().String()) +
		`,"attributes":` + otlpAttributes(attrs) + `,"body":` + otlpValue(body) + `,"droppedAttributesCount":0`
	if s, ok := some(currentSpan(fb)); ok {
		out += `,"traceId":` + QuoteJSON(otlpTraceId(spanOf(s).traceId)) + `,"spanId":` + QuoteJSON(spanOf(s).spanId)
	}
	return out + "}"
}

func otlpLogs(resource string, records []string) string {
	return `{"resourceLogs":[{"resource":` + resource + `,"scopeLogs":[{"scope":` + otlpScope +
		`,"logRecords":[` + strings.Join(records, ",") + `]}]}]}`
}

var otlpUnits = map[string]string{"nanoseconds": "ns", "microseconds": "us", "milliseconds": "ms", "seconds": "s", "bytes": "By"}

// otlpMetric is a metric of the snapshot: its name, description and unit,
// its kind, the fields before its points and its points.
type otlpMetric struct {
	head, kind, fields string
	points             []string
}

// otlpMetrics is the snapshot of every series, made at `now`, for an
// exporter started at `start`, all cumulative; the series of one name are
// the points of one metric.
func otlpMetrics(resource, service string, start, now *big.Int, snapshot []metricSnapshot) string {
	var metrics []*otlpMetric
	byName := map[string]*otlpMetric{}
	times := `"startTimeUnixNano":` + QuoteJSON(start.String()) + `,"timeUnixNano":` + QuoteJSON(now.String())
	for _, s := range snapshot {
		var attrs []spanAttr
		unit := "1"
		if s.attributes != nil {
			for i, k := range s.attributes.keys {
				attrs = append(attrs, spanAttr{k, s.attributes.vals[i]})
			}
			u, ok := recordField(s.attributes, "unit").(string)
			if !ok {
				u, ok = recordField(s.attributes, "time_unit").(string)
			}
			if ok {
				unit = u
				if ucum, known := otlpUnits[u]; known {
					unit = ucum
				}
			}
		}
		point := `{"attributes":` + otlpAttributes(attrs) + `,` + times
		n := func(k string) string { return jsonNumber(s.state.Get(k).(float64)) }
		var kind, fields string
		var points []string
		switch s.kind {
		case "Counter":
			kind, fields = "sum", `"aggregationTemporality":2,"isMonotonic":`+strconv.FormatBool(s.state.Get("incremental").(bool))+`,`
			points = []string{point + `,"asDouble":` + n("count") + "}"}
		case "Gauge":
			kind, points = "gauge", []string{point + `,"asDouble":` + n("value") + "}"}
		case "Histogram":
			var bounds, counts []string
			prev := 0.0
			for _, b := range List(s.state.Get("buckets")) {
				boundary := At(b, 0).(float64)
				if math.IsInf(boundary, 1) {
					break
				}
				bounds = append(bounds, jsonNumber(boundary))
				counts = append(counts, jsonNumber(At(b, 1).(float64)-prev))
				prev = At(b, 1).(float64)
			}
			counts = append(counts, jsonNumber(s.state.Get("count").(float64)-prev))
			kind, fields = "histogram", `"aggregationTemporality":2,`
			points = []string{point + `,"count":` + n("count") + `,"min":` + n("min") + `,"max":` + n("max") + `,"sum":` + n("sum") +
				`,"bucketCounts":[` + strings.Join(counts, ",") + `],"explicitBounds":[` + strings.Join(bounds, ",") + "]}"}
		case "Frequency":
			kind, fields = "sum", `"aggregationTemporality":2,"isMonotonic":true,`
			for _, kv := range List(s.state.Get("occurrences")) {
				keyed := otlpAttributes(append(append([]spanAttr{}, attrs...), spanAttr{"key", At(kv, 0)}))
				points = append(points, `{"attributes":`+keyed+`,`+times+`,"asInt":`+jsonNumber(At(kv, 1).(float64))+"}")
			}
		case "Summary":
			var quantiles []string
			for _, q := range List(s.state.Get("quantiles")) {
				if v, ok := some(At(q, 1)); ok {
					quantiles = append(quantiles, `{"quantile":`+jsonNumber(At(q, 0).(float64))+`,"value":`+jsonNumber(v.(float64))+"}")
				}
			}
			kind = "summary"
			points = []string{point + `,"count":` + n("count") + `,"sum":` + n("sum") + `,"quantileValues":[` + strings.Join(quantiles, ",") + "]}"}
		}
		if m, ok := byName[s.id]; ok {
			m.points = append(m.points, points...)
			continue
		}
		head := `"name":` + QuoteJSON(s.id)
		if d, ok := some(s.description); ok {
			head += `,"description":` + QuoteJSON(d.(string))
		}
		m := &otlpMetric{head + `,"unit":` + QuoteJSON(unit), kind, fields, points}
		byName[s.id] = m
		metrics = append(metrics, m)
	}
	encoded := make([]string, len(metrics))
	for i, m := range metrics {
		encoded[i] = "{" + m.head + `,"` + m.kind + `":{` + m.fields + `"dataPoints":[` + strings.Join(m.points, ",") + "]}}"
	}
	return `{"resourceMetrics":[{"resource":` + resource + `,"scopeMetrics":[{"scope":{"name":` + QuoteJSON(service) +
		`},"metrics":[` + strings.Join(encoded, ",") + `]}]}]}`
}
