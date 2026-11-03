package flickrt

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

// TestCliTraceExports runs a command under `--trace` against a collector
// and reads what each of the three endpoints got.
func TestCliTraceExports(t *testing.T) {
	var mu sync.Mutex
	bodies, agents := map[string]string{}, map[string]string{}
	collector := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		mu.Lock()
		bodies[r.URL.Path] += string(body)
		agents[r.URL.Path] = r.Header.Get("user-agent")
		mu.Unlock()
		if !json.Valid(body) {
			t.Errorf("%s got invalid JSON: %s", r.URL.Path, body)
		}
		_, _ = w.Write([]byte("{}"))
	}))
	defer collector.Close()
	t.Setenv("OTEL_EXPORTER_OTLP_ENDPOINT", collector.URL)
	t.Setenv("OTEL_SERVICE_NAME", "shop")
	orders := newCounter("cli_otlp_test_orders", Some("Orders placed"), nil, true)
	_, cause := catch(func() Value {
		return cliTraced(rootFiber(), "checkout", func(fb *Fiber) Value {
			orders.update(metricContextOf(fb), 2.0)
			logMessage(fb, "Warn", "low stock")
			return TracerWithSpan(fb, "pay", Rec("parent", None, "root", false), Eff(func(*Fiber) Value { return Fail("declined") }))
		})
	})
	if cause == nil {
		t.Fatal("the command's failure is lost")
	}
	mu.Lock()
	defer mu.Unlock()
	for path, wants := range map[string][]string{
		"/v1/traces": {`"name":"pay"`, `"name":"checkout"`, `"status":{"code":2,"message":"\"declined\""}`,
			`{"name":"exception","timeUnixNano":`, `"exception.type","value":{"stringValue":"Error"}`, `"exception.stacktrace"`},
		"/v1/logs": {`"severityNumber":13,"severityText":"Warn"`, `"body":{"stringValue":"low stock"}`,
			`"key":"effect.fiberId"`, `"traceId":`, `"scope":{"name":"effect","version":"4.1.2"}`},
		"/v1/metrics": {`"scope":{"name":"shop"}`, `"name":"cli_otlp_test_orders","description":"Orders placed","unit":"1"`,
			`"sum":{"aggregationTemporality":2,"isMonotonic":true,"dataPoints":[`, `"asDouble":2}`},
	} {
		for _, want := range wants {
			if !strings.Contains(bodies[path], want) {
				t.Errorf("%s lacks %s: %s", path, want, bodies[path])
			}
		}
	}
	if want := "OTel-OTLP-Exporter-JavaScript-Effect-OtlpLogger/4.1.2"; agents["/v1/logs"] != want {
		t.Errorf("user agent %q, want %q", agents["/v1/logs"], want)
	}
}

// TestCliTraceShutdownTimeout ends a command whose collector never
// answers after the shutdown timeout, not later.
func TestCliTraceShutdownTimeout(t *testing.T) {
	release := make(chan struct{})
	collector := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { <-release }))
	defer collector.Close()
	defer close(release)
	t.Setenv("OTEL_EXPORTER_OTLP_ENDPOINT", collector.URL)
	started := time.Now()
	cliTraced(rootFiber(), "stuck", func(*Fiber) Value { return Unit })
	if took := time.Since(started); took < otlpShutdownTimeout || took > otlpShutdownTimeout+time.Second {
		t.Errorf("the command ended after %v, want the shutdown timeout %v", took, otlpShutdownTimeout)
	}
}
