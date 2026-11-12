package flickrt

import (
	"bufio"
	"bytes"
	"fmt"
	"math"
	"math/big"
	"strings"
	"testing"
	"time"
)

// A property's case reads as the JS runner's JSON.stringify writes it.
func TestPreviewWritesCasesAsTheJSRunner(t *testing.T) {
	for _, c := range []struct {
		value Value
		want  string
	}{
		{[]Value{math.NaN(), math.Inf(-1), math.Copysign(0, -1), 1.5}, "[null,null,0,1.5]"},
		{[]Value{big.NewInt(-5), Unit}, `["-5n",null]`},
		{Rec("a", Unit, "b", int32(1)), `{"b":1}`},
		{[]Value{Some(int32(1)), None}, `[{"_id":"Option","_tag":"Some","value":1},{"_id":"Option","_tag":"None"}]`},
		{Err("e"), `{"_id":"Result","_tag":"Failure","failure":"e"}`},
		{Fn(func(fb *Fiber, v Value) Value { return v }), `"<function>"`},
		{Unit, "undefined"},
	} {
		if got := preview(c.value); got != c.want {
			t.Errorf("preview = %s, want %s", got, c.want)
		}
	}
}

func TestParseTestArgsColor(t *testing.T) {
	for _, mode := range []string{"", "always", "never", "auto"} {
		args := []string{"--watched"}
		if mode != "" {
			args = append(args, "--color", mode)
		}
		options := parseTestArgs(args)
		if options.color != (mode == "always") || !options.watched {
			t.Errorf("color %q: color=%v watched=%v", mode, options.color, options.watched)
		}
	}
}

func TestParseTestArgsProgress(t *testing.T) {
	if !parseTestArgs([]string{"--progress", "live"}).progress {
		t.Fatal("live progress was not enabled")
	}
	if parseTestArgs([]string{"--progress", "plain"}).progress {
		t.Fatal("plain progress enabled live rendering")
	}
}

func TestWriteTestResultFlushes(t *testing.T) {
	outMu.Lock()
	previous := stdout
	previous.Flush()
	defer func() {
		stdout = previous
		outMu.Unlock()
	}()
	var output bytes.Buffer
	stdout = bufio.NewWriter(&output)
	for _, test := range []struct {
		name     string
		result   testResult
		color    bool
		want     []string
		unwanted string
	}{
		{
			name:     "passed",
			result:   testResult{Name: "App.Test > first", Status: "passed"},
			want:     []string{"  ✓ [1/2] App.Test > first", "(2 ms)\n"},
			unwanted: "\x1b",
		},
		{
			name:   "color",
			result: testResult{Name: "App.Test > first", Status: "passed"},
			color:  true,
			want:   []string{"\x1b[32m✓\x1b[0m", "\x1b[2m[1/2]\x1b[0m", "\x1b[2m(2 ms)\x1b[0m\n"},
		},
		{
			name:   "failure",
			result: testResult{Name: "App.Test > first", Status: "failed", Kind: "assertion", Message: "first line\nsecond line", Cleanup: "cleanup first\ncleanup second"},
			color:  true,
			want:   []string{"\x1b[31m✗\x1b[0m", "    assertion: first line\n    second line\n", "    and cleanup failed: cleanup first\n    cleanup second\n"},
		},
		{
			name:     "cleanup",
			result:   testResult{Name: "App.Test > first", Status: "failed", Kind: "cleanup", Message: "cleanup failed", Cleanup: "cleanup failed"},
			want:     []string{"  ✗ [1/2] App.Test > first", "    cleanup: cleanup failed\n"},
			unwanted: "and cleanup failed:",
		},
	} {
		t.Run(test.name, func(t *testing.T) {
			output.Reset()
			writeTestResult(test.result, 1, 2, 2*time.Millisecond, test.color)
			got := output.String()
			for _, want := range test.want {
				if !strings.Contains(got, want) {
					t.Errorf("flushed output %q does not contain %q", got, want)
				}
			}
			if test.unwanted != "" && strings.Contains(got, test.unwanted) {
				t.Errorf("flushed output %q contains %q", got, test.unwanted)
			}
			if !strings.HasSuffix(got, "\n") {
				t.Errorf("flushed output is incomplete: %q", got)
			}
		})
	}
}

// A supervisor that fails during a test fails that test, as it fails a
// program, and not the run of another one.
func TestASupervisorThatFailsFailsItsTest(t *testing.T) {
	broken := Fn(func(*Fiber, Value) Value { panic(Defect{"Error: broken"}) })
	task := Eff(func(fb *Fiber) Value {
		return Scope(fb, func(fb *Fiber, owner, _ Value) Value {
			return FiberSpawn(fb, owner, broken, Eff(func(*Fiber) Value { panic(Defect{"Error: child"}) }))
		})
	})
	before := len(rootDefects.causes)
	got := runOne(rootFiber(), named{"dies", &test{name: "dies", task: task}, nil}, 0, nil)
	if got.Status != "failed" || got.Kind != "defect" {
		t.Errorf("the test fails with the supervisor's defect: %+v", got)
	}
	if len(rootDefects.causes) != before {
		t.Error("the defect stays with the test, not the program")
	}
}

// A suite builds its layer once, before its first test, and releases it
// after its last; its tests share what it built.
func TestSuiteSharesItsLayer(t *testing.T) {
	var events []string
	layer := &Layer{func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		events = append(events, "acquire")
		scope.add(fb, func(*Fiber, *Cause) Value {
			events = append(events, "release")
			return Unit
		})
		return Services{"App.Box": "shared"}
	}}
	read := func(name string) Value {
		return Eff(func(fb *Fiber) Value {
			box, _ := fb.env.get("App.Box")
			events = append(events, name+" "+box.(string))
			return Unit
		})
	}
	suite := TestSuite("suite", layer, []Value{TestEffect("a", read("a")), TestEffect("b", read("b"))})
	selected := flatten("M", []Value{suite}, nil, nil)
	runs := suiteRuns(rootFiber(), selected)
	for _, n := range selected {
		if got := runOne(rootFiber(), n, 0, runs); got.Status != "passed" {
			t.Fatalf("%s: %+v", n.name, got)
		}
	}
	want := []string{"acquire", "a shared", "b shared", "release"}
	if fmt.Sprint(events) != fmt.Sprint(want) {
		t.Errorf("events %v, want %v", events, want)
	}
}
