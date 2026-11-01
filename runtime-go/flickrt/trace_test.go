package flickrt

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// A traced scope writes its open, the finalizer Flick adds, its close
// phases and the finalizer's run, one JSON object per line.
func TestATracedScopeWritesItsSteps(t *testing.T) {
	path := filepath.Join(t.TempDir(), "trace")
	tracePath, tracing, traceFile = path, true, nil
	defer func() {
		_ = traceFile.Close()
		tracePath, tracing, traceFile = "", false, nil
	}()
	fb := rootFiber()
	Scope(fb, func(fb *Fiber, _, _ Value) Value {
		return AddFinalizer(fb, Eff(func(*Fiber) Value { return Unit }))
	})
	text, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(strings.TrimSpace(string(text)), "\n")
	want := []string{`{"e":"open","o":`, `{"e":"fin",`, `"c":"C0"}`, `"c":"C5"}`, `{"e":"run",`, `"c":"C6"}`}
	at := 0
	for _, line := range lines {
		if at < len(want) && strings.Contains(line, want[at]) {
			at++
		}
	}
	if at != len(want) || !strings.HasSuffix(lines[0], `"p":0,"k":"scope"}`) {
		t.Errorf("the steps in order: found %d of %d in\n%s", at, len(want), text)
	}
}
