package flickrt

import (
	"bytes"
	"net/http"
	"os"
	"strconv"
	"time"
)

// The built-in behaviors of `--wizard` and `--trace`.

// cliWizard is the wizard mode of Command.runWith.
// shortcut: the wizard is not ported; `--wizard` is an unrecognized flag until internal/wizard.ts is
func cliWizard(fb *Fiber, root *cliCommand, args []string, ctx *cliContext) *cliError {
	return cliUnrecognized("--wizard", ctx.path, []string{})
}

// cliTraced is run.ts's `traced`: under `--trace` the handler of command
// `name` runs as a span of that name, sent over OTLP/HTTP JSON to
// OTEL_EXPORTER_OTLP_ENDPOINT as service OTEL_SERVICE_NAME when it ends.
// shortcut: only the command's span is exported, not its child spans, logs or metrics; port Effect's Otlp tracer, logger and metrics when a program reads them
func cliTraced(fb *Fiber, name string, run func(*Fiber) Value) Value {
	s := openSpan(fb, name, Rec("parent", None, "root", false))
	start := time.Now()
	out, cause := catch(func() Value { return withEnv(fb, parentSpanKey, s, Eff(run)) })
	end := time.Now()
	endpoint, service := os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT"), os.Getenv("OTEL_SERVICE_NAME")
	if endpoint == "" {
		endpoint = "http://localhost:4318"
	}
	if service == "" {
		service = name
	}
	status := 1 // Ok
	if cause != nil {
		status = 2 // Error
	}
	body := cliOtlpSpans(service, s, start, end, status)
	if response, err := http.Post(endpoint+"/v1/traces", "application/json", bytes.NewReader([]byte(body))); err == nil {
		response.Body.Close()
	}
	if cause != nil {
		panic(&Failure{cause})
	}
	return out
}

// cliOtlpSpans is an ExportTraceServiceRequest of one span, in OTLP's JSON.
func cliOtlpSpans(service string, s *span, start, end time.Time, status int) string {
	parent := ""
	if p, ok := some(s.parent); ok {
		parent = `"parentSpanId":` + QuoteJSON(spanOf(p).spanId) + `,`
	}
	nanos := func(t time.Time) string { return QuoteJSON(strconv.FormatInt(t.UnixNano(), 10)) }
	name, _ := some(s.name)
	return `{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":` + QuoteJSON(service) +
		`}}],"droppedAttributesCount":0},"scopeSpans":[{"scope":{"name":` + QuoteJSON(service) + `},"spans":[{"traceId":` +
		QuoteJSON(s.traceId) + `,"spanId":` + QuoteJSON(s.spanId) + `,` + parent + `"name":` + QuoteJSON(name.(string)) +
		`,"kind":1,"startTimeUnixNano":` + nanos(start) + `,"endTimeUnixNano":` + nanos(end) +
		`,"attributes":[],"droppedAttributesCount":0,"events":[],"droppedEventsCount":0,"status":{"code":` +
		strconv.Itoa(status) + `},"links":[],"droppedLinksCount":0}]}]}]}`
}
