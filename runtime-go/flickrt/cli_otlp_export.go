package flickrt

import (
	"context"
	"errors"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"
)

// Effect's OtlpExporter.make, for `--trace`: the items pushed are buffered
// and posted as one batch every interval, as soon as `maxBatch` are
// waiting, and once more when the command ends, within the shutdown
// timeout. A failed post disables the exporter for a minute and drops its
// buffer; a transient one is retried three times first.
type otlpExporter struct {
	url, label  string
	maxBatch    int  // 0: no limit
	exportEmpty bool // posts even an empty batch, as the metrics do
	body        func(items []string) string

	mu            sync.Mutex
	buffer        []string
	disabledUntil time.Time // zero: enabled
	exports       sync.WaitGroup
	ctx           context.Context // ends the posts once the command has ended
	cancel        context.CancelFunc
	stop          chan struct{}
}

const (
	otlpShutdownTimeout = 3 * time.Second
	otlpDisabledFor     = 60 * time.Second
)

func newOtlpExporter(url, label string, interval time.Duration, maxBatch int, exportEmpty bool, body func([]string) string) *otlpExporter {
	e := &otlpExporter{url: url, label: label, maxBatch: maxBatch, exportEmpty: exportEmpty, body: body, stop: make(chan struct{})}
	e.ctx, e.cancel = context.WithCancel(context.Background())
	e.exports.Add(1) // the interval's posts are awaited at shutdown too
	go func() {
		defer e.exports.Done()
		timer := time.NewTimer(interval)
		defer timer.Stop()
		for {
			select {
			case <-e.stop:
				return
			case <-timer.C:
				e.export()
				timer.Reset(interval)
			}
		}
	}()
	return e
}

func (e *otlpExporter) push(item string) {
	e.mu.Lock()
	if !e.disabledUntil.IsZero() {
		e.mu.Unlock()
		return
	}
	e.buffer = append(e.buffer, item)
	full := e.maxBatch > 0 && len(e.buffer) >= e.maxBatch
	e.mu.Unlock()
	if full {
		e.fork()
	}
}

func (e *otlpExporter) fork() {
	e.exports.Add(1)
	go func() {
		defer e.exports.Done()
		e.export()
	}()
}

// export posts the buffer as one batch, unless the exporter is disabled.
func (e *otlpExporter) export() {
	e.mu.Lock()
	if !e.disabledUntil.IsZero() {
		if time.Now().Before(e.disabledUntil) {
			e.mu.Unlock()
			return
		}
		e.disabledUntil = time.Time{}
	}
	if len(e.buffer) == 0 && !e.exportEmpty {
		e.mu.Unlock()
		return
	}
	items := e.buffer
	e.buffer = nil
	e.mu.Unlock()
	if otlpPost(e.ctx, e.url, e.label, e.body(items)) != nil {
		e.mu.Lock()
		if e.disabledUntil.IsZero() {
			e.disabledUntil, e.buffer = time.Now().Add(otlpDisabledFor), nil
		}
		e.mu.Unlock()
	}
}

// shutdown stops the interval and posts what is left, waiting for the
// posts under way at most the shutdown timeout.
func (e *otlpExporter) shutdown() {
	close(e.stop)
	defer e.cancel()
	e.mu.Lock()
	disabled := !e.disabledUntil.IsZero()
	e.mu.Unlock()
	if disabled {
		return
	}
	e.fork()
	done := make(chan struct{})
	go func() {
		e.exports.Wait()
		close(done)
	}()
	select {
	case <-done:
	case <-time.After(otlpShutdownTimeout):
	}
}

var errOtlpStatus = errors.New("OTLP collector answered a status that is not ok")

// otlpPost posts the JSON body, retrying a transport failure or a transient
// status three times: after a second, or what a 429's Retry-After asks.
func otlpPost(ctx context.Context, url, label, body string) error {
	for attempt := 0; ; attempt++ {
		request, err := http.NewRequestWithContext(ctx, http.MethodPost, url, strings.NewReader(body))
		if err != nil {
			return err
		}
		request.Header.Set("content-type", "application/json")
		request.Header.Set("user-agent", "OTel-OTLP-Exporter-JavaScript-Effect-"+label+"/4.0.5")
		response, err := http.DefaultClient.Do(request)
		delay := time.Second
		if err == nil {
			_, _ = io.Copy(io.Discard, response.Body)
			response.Body.Close()
			status := int32(response.StatusCode)
			if status >= 200 && status < 300 {
				return nil
			}
			if !transientStatus[status] {
				return errOtlpStatus
			}
			if status == 429 {
				delay = 5 * time.Second
				if d := retryAfter(float64(time.Now().UnixMilli()), response.Header.Get("retry-after")); d != nil {
					delay = time.Duration(millisOf(d) * float64(time.Millisecond))
				}
			}
			err = errOtlpStatus
		}
		if attempt == 3 || ctx.Err() != nil {
			return err
		}
		select {
		case <-time.After(delay):
		case <-ctx.Done():
			return ctx.Err()
		}
	}
}
