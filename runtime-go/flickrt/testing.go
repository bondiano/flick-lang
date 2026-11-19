package flickrt

import (
	"fmt"
	"io"
	"math"
	"math/big"
	"math/rand/v2"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Tests and generators, as the JS runtime's testing module: a test is data
// built by pure code; RunTests runs them, each in an owner of its own, and
// reports assertions, typed failures, defects and failed cleanup apart,
// with the JS runner's output.

var (
	passTag      = Tag("Test.Assertion.Pass")
	assertionTag = Tag("Test.AssertionFailed.AssertionFailed")
)

// rng is a number in [0, 1); a gen draws a value from one.
type rng func() float64
type gen func(*Fiber, rng) Value

// random is mulberry32, the JS runner's generator, so a seed draws the
// same values on both targets.
func random(seed uint32) rng {
	a := seed
	return func() float64 {
		a += 0x6d2b79f5
		t := a
		t = (t ^ t>>15) * (t | 1)
		t ^= t + (t^t>>7)*(t|61)
		return float64(t^t>>14) / 4294967296
	}
}

func GenConstant(v Value) Value { return gen(func(fb *Fiber, _ rng) Value { return v }) }

func GenNumber(low, high Value) Value {
	l, h := low.(float64), high.(float64)
	return gen(func(fb *Fiber, r rng) Value { return l + r()*(h-l) })
}

func GenInt(low, high Value) Value {
	l, h := low.(int32), high.(int32)
	return gen(func(fb *Fiber, r rng) Value { return l + int32(math.Floor(r()*float64(h-l+1))) })
}

var GenBool Value = gen(func(fb *Fiber, r rng) Value { return r() < 0.5 })

func GenString(max Value) Value {
	m := float64(max.(int32))
	return gen(func(fb *Fiber, r rng) Value {
		b := make([]byte, int(math.Floor(r()*(m+1))))
		for i := range b {
			b[i] = byte(32 + math.Floor(r()*95))
		}
		return string(b)
	})
}

func GenList(max, g Value) Value {
	m, each := float64(max.(int32)), g.(gen)
	return gen(func(fb *Fiber, r rng) Value {
		out := make([]Value, int(math.Floor(r()*(m+1))))
		for i := range out {
			out[i] = each(fb, r)
		}
		return out
	})
}

func GenOneOf(values Value) Value {
	vs := List(values)
	if len(vs) == 0 {
		panic(Defect{"RangeError: Gen.one_of needs at least one value"})
	}
	return gen(func(fb *Fiber, r rng) Value { return vs[int(math.Floor(r()*float64(len(vs))))] })
}

func GenPair(a, b Value) Value {
	ga, gb := a.(gen), b.(gen)
	return gen(func(fb *Fiber, r rng) Value { return []Value{ga(fb, r), gb(fb, r)} })
}

func GenMap(f, g Value) Value {
	each := g.(gen)
	return gen(func(fb *Fiber, r rng) Value { return Apply(fb, f, each(fb, r)) })
}

// GenFunction is a pure function whose result for an argument is drawn
// with a seed of the argument's hash, the same each call.
func GenFunction(fb *Fiber, d, g Value) Value {
	hash, each := Method(fb, d, "hash"), g.(gen)
	return gen(func(fb *Fiber, r rng) Value {
		seed := uint32(math.Floor(r() * 4294967296))
		return Fn(func(fb *Fiber, a Value) Value { return each(fb, random(seed^uint32(Apply(fb, hash, a).(int32)))) })
	})
}

// A test: a pure body, an effectful task, a property, a group, or a
// suite: a group whose tests get the services of its layer.
type test struct {
	name  string
	body  Value // pure: Unit -> Assertion
	task  Value // effect
	gen   gen   // property, with its check
	check Value
	group []Value
	layer Value // suite
	// "skip" or "only", of Test.skip and Test.only: for it and each test in it
	mode string
	// the kind of a hook, "before_all", "before_each", "after_each" or
	// "after_all", whose task is `task`; the hooks of a group with them
	when  string
	hooks *groupHooks
	// the milliseconds of Test.timeout, for it and each test in it without its own
	timeout float64
	timed   bool
}

func TestPure(name, body Value) Value   { return &test{name: name.(string), body: body} }
func TestEffect(name, task Value) Value { return &test{name: name.(string), task: task} }
func TestGroup(name, tests Value) Value { return &test{name: name.(string), group: List(tests)} }

func TestSuite(name, layer, tests Value) Value {
	return &test{name: name.(string), group: List(tests), layer: layer}
}

func TestProperty(name, g, check Value) Value {
	return &test{name: name.(string), gen: g.(gen), check: check}
}

// failed is the message of a failed assertion, "" for a pass.
func failed(a Value) (string, bool) {
	d := a.(*Data)
	if d.Tag == passTag {
		return "", false
	}
	return d.Fields()[0].(string), true
}

type named struct {
	name string
	test *test
	// the suites it is in, outermost first: those of Test.suite and the
	// groups with hooks
	suites []*test
	marks
}

// marks are what a test inherits from the groups it is in: whether it or
// one of them is skipped, or marked only, and the innermost timeout.
type marks struct {
	skip, only bool
	timeout    float64
	timed      bool
}

// flatten is the tests with their full names, `Module > group > name`; the
// hooks of a list make it, the module's or a group's, a suite of the tests
// under it.
func flatten(prefix string, tests []Value, out []named, suites []*test, m marks) []named {
	if h := hooksOf(tests); h != nil {
		suites = append(suites[:len(suites):len(suites)], &test{name: prefix, hooks: h})
	}
	for _, v := range tests {
		t := v.(*test)
		if t.when != "" {
			continue
		}
		name := prefix + " > " + t.name
		own := marks{skip: m.skip || t.mode == "skip", only: m.only || t.mode == "only", timeout: m.timeout, timed: m.timed}
		if t.timed {
			own.timeout, own.timed = t.timeout, true
		}
		switch {
		case t.layer != nil:
			out = flatten(name, t.group, out, append(suites[:len(suites):len(suites)], t), own)
		case t.group != nil:
			out = flatten(name, t.group, out, suites, own)
		default:
			out = append(out, named{name, t, suites, own})
		}
	}
	return out
}

// testResult is the result of one test, as the reporters write it.
type testResult struct {
	Name    string `json:"name"`
	Status  string `json:"status"`
	Kind    string `json:"kind,omitempty"`
	Message string `json:"message,omitempty"`
	Cleanup string `json:"cleanup,omitempty"`
}

const testCases = 100

// checks runs a pure or property test: a failed assertion's message.
func (t *test) checks(fb *Fiber, seed int64) (string, bool) {
	if t.body != nil {
		return failed(Apply(fb, t.body, Unit))
	}
	for i := range testCases {
		value := t.gen(fb, random(uint32(seed+int64(i)*0x9e3779b9)))
		if message, bad := failed(Apply(fb, t.check, value)); bad {
			return fmt.Sprintf("%s (case %d: %s)", message, i, preview(value)), true
		}
	}
	return "", false
}

// preview is a property's case as the JS runner writes it: JSON.stringify
// with a BigInt as "<digits>n", the toJSON of Option, Result and Redacted,
// and a function as "<function>"; `undefined` alone as itself.
func preview(v Value) string {
	if s, ok := previewJSON(v); ok {
		return s
	}
	return "undefined"
}

// previewJSON is the JSON of a value, or false for `undefined`, which an
// array writes as null and an object leaves out.
func previewJSON(v Value) (string, bool) {
	switch x := asData(v).(type) {
	case UnitType:
		return "", false
	case float64:
		switch {
		case math.IsNaN(x) || math.IsInf(x, 0):
			return "null", true
		case x == 0:
			return "0", true // -0 too
		}
		return ShowNumber(x), true
	case *big.Int:
		return QuoteJSON(x.String() + "n"), true
	case Fn, Fn2, Fn3, Poly:
		return `"<function>"`, true
	case *Redacted:
		return `"<redacted>"`, true // its toJSON
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			if parts[i], _ = previewJSON(e); parts[i] == "" {
				parts[i] = "null"
			}
		}
		return "[" + strings.Join(parts, ",") + "]", true
	case *Record:
		return previewObject(x.keys, x.vals), true
	case *Data:
		switch x.Tag {
		case "Some", "None":
			return previewObject(append([]string{"_id", "_tag"}, fieldNames(x)...), append([]Value{"Option", x.Tag}, x.Fields()...)), true
		case "Success":
			return previewObject([]string{"_id", "_tag", "value"}, []Value{"Result", x.Tag, x.Fields()[0]}), true
		case "Failure":
			return previewObject([]string{"_id", "_tag", "failure"}, []Value{"Result", x.Tag, x.Fields()[0]}), true
		}
		return previewObject(append([]string{"_tag"}, fieldNames(x)...), append([]Value{x.Tag}, x.Fields()...)), true
	}
	return Format(v), true
}

// fieldNames is the JS property of each field of a variant value.
func fieldNames(d *Data) []string {
	out := make([]string, len(d.Fields()))
	for i := range d.Fields() {
		out[i] = nativeField(d.Tag, i)
	}
	return out
}

func previewObject(keys []string, vals []Value) string {
	var out strings.Builder
	out.WriteByte('{')
	for i, k := range keys {
		s, ok := previewJSON(vals[i])
		if !ok {
			continue
		}
		if out.Len() > 1 {
			out.WriteByte(',')
		}
		out.WriteString(QuoteJSON(k) + ":" + s)
	}
	out.WriteByte('}')
	return out.String()
}

// describeText is a payload as the JS runner's describeCause shows it.
func describeText(v Value) string {
	switch x := v.(type) {
	case string:
		if len(x) > 200 {
			return x[:200] + "…"
		}
		return x
	case float64:
		return ShowNumber(x)
	case int32:
		return strconv.Itoa(int(x))
	case bool:
		return strconv.FormatBool(x)
	case UnitType:
		return "undefined"
	case Constructed:
		return ConstructorName(x.TagName())
	}
	return "<object>"
}

// describeReasons is the JS runner's describeCause of these reasons.
func describeReasons(reasons []*Cause) string {
	parts := make([]string, len(reasons))
	for i, r := range reasons {
		switch r.kind {
		case causeFail:
			parts[i] = "fail " + describeText(r.err)
		case causeDie:
			message, _, _ := strings.Cut(r.defect, "\n")
			if name, rest, ok := strings.Cut(message, ": "); ok && strings.HasSuffix(name, "Error") {
				message = rest
			}
			parts[i] = "die error " + message
		default:
			parts[i] = "interrupt"
		}
	}
	return strings.Join(parts, "; ")
}

// outcome is how a test body ended: its kind and message, "" when it passed.
func outcome(message string, bad bool, cause *Cause) (string, string) {
	switch {
	case cause == nil && !bad:
		return "", ""
	case cause == nil:
		return "assertion", message
	case cause.has(causeDie):
		return "defect", describeReasons(cause.reasons())
	case !cause.has(causeFail):
		return "interrupted", "interrupted"
	}
	if r := cause.firstOf(causeFail); r != nil {
		if d, ok := r.err.(*Data); ok && d.Tag == assertionTag {
			return "assertion", d.Fields()[0].(string)
		}
	}
	return "failure", describeReasons(cause.reasons())
}

// writeTestResult writes and flushes a complete result with outMu held by the caller.
func writeTestResult(r testResult, completed, total int, elapsed time.Duration, color bool) {
	marker := "✓"
	markerColor, dim, reset := "", "", ""
	if r.Status != "passed" {
		marker = "✗"
	}
	if color {
		markerColor, dim, reset = "\x1b[32m", "\x1b[2m", "\x1b[0m"
		if r.Status != "passed" {
			markerColor = "\x1b[31m"
		}
	}
	fmt.Fprintf(stdout, "  %s%s%s %s[%d/%d]%s %s  %s(%d ms)%s\n", markerColor, marker, reset, dim, completed, total, reset, r.Name, dim, elapsed.Milliseconds(), reset)
	if r.Status != "passed" {
		fmt.Fprintf(stdout, "    %s: %s\n", r.Kind, strings.ReplaceAll(r.Message, "\n", "\n    "))
		if r.Cleanup != "" && r.Kind != "cleanup" {
			fmt.Fprintf(stdout, "    and cleanup failed: %s\n", strings.ReplaceAll(r.Cleanup, "\n", "\n    "))
		}
	}
	stdout.Flush()
}

func reportTests(results, skipped []testResult, seed int64, reporter string, color bool, elapsed time.Duration) int {
	failed := 0
	for _, r := range results {
		if r.Status == "failed" {
			failed++
		}
	}
	passed := len(results) - failed
	if reporter == "json" {
		var tests []string
		for _, r := range append(results[:len(results):len(results)], skipped...) {
			tests = append(tests, jsonOf(r))
		}
		// a run without skipped tests reports as before they existed
		counts := fmt.Sprintf("\"seed\":%d,\"passed\":%d,\"failed\":%d", seed, passed, failed)
		if len(skipped) > 0 {
			counts += fmt.Sprintf(",\"skipped\":%d", len(skipped))
		}
		fmt.Fprintf(stdout, "{%s,\"tests\":[%s]}\n", counts, strings.Join(tests, ","))
		return failed
	}
	passColor, failColor, dim, reset := "", "", "", ""
	if color {
		dim, reset = "\x1b[2m", "\x1b[0m"
		passColor, failColor = dim, dim
		if passed > 0 {
			passColor = "\x1b[32m"
		}
		if failed > 0 {
			failColor = "\x1b[31m"
		}
	}
	also := ""
	if len(skipped) > 0 {
		fmt.Fprintln(stdout)
		for _, r := range skipped {
			fmt.Fprintf(stdout, "  %s- %s  (skipped)%s\n", dim, r.Name, reset)
		}
		also = fmt.Sprintf(" · %s%d%s skipped", dim, len(skipped), reset)
	}
	fmt.Fprintf(stdout, "\n  Tests  %s%d%s passed · %s%d%s failed%s · %d total\n", passColor, passed, reset, failColor, failed, reset, also, len(results)+len(skipped))
	fmt.Fprintf(stdout, "  Time   %s%d ms%s\n", dim, elapsed.Milliseconds(), reset)
	if failed > 0 {
		fmt.Fprintf(stdout, "seed: %d (rerun with --seed %d)\n", seed, seed)
	}
	return failed
}

// jsonOf is a result as JSON.stringify writes it, keys in its order.
func jsonOf(r testResult) string {
	out := "{\"name\":" + QuoteJSON(r.Name) + ",\"status\":" + QuoteJSON(r.Status)
	for _, f := range [][2]string{{"kind", r.Kind}, {"message", r.Message}, {"cleanup", r.Cleanup}} {
		if f[1] != "" {
			out += "," + QuoteJSON(f[0]) + ":" + QuoteJSON(f[1])
		}
	}
	return out + "}"
}

// Suite is the `tests` of a test module.
type Suite struct {
	Module string
	Tests  Value
}

// RunTests runs the tests of each suite, at most `--jobs` at a time, and
// exits: 0 when all passed, 1 otherwise, 130 when SIGINT (or, for a
// `--watched` run, the end of stdin) stopped the run after every running
// test's cleanup.
func RunTests(fb *Fiber, suites []Suite) {
	o := parseTestArgs(os.Args[1:])
	seed := o.seed
	if !o.seeded {
		seed = rand.Int64N(1 << 31)
	}
	selected, skipped := selectTests(suites, o)
	if o.watched {
		// a watcher stops the run by closing stdin, which works on every platform
		go func() {
			io.Copy(io.Discard, os.Stdin)
			fb.state.interrupt(0)
		}()
	}
	runs := suiteRuns(fb, selected)
	results := make([]testResult, len(selected))
	slots := make(chan struct{}, o.jobs)
	var wg sync.WaitGroup
	human := o.reporter != "json"
	live := human && o.progress
	completed := 0
	var started time.Time
	active := make(map[int]string)
	frames := []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}
	frame := 0
	clear := func() {
		if live {
			fmt.Fprint(stdout, "\r\x1b[2K")
			stdout.Flush()
		}
	}
	draw := func() {
		if !live || len(active) == 0 {
			return
		}
		name := ""
		for i := range selected {
			if current, ok := active[i]; ok {
				name = current
				break
			}
		}
		runes := []rune(name)
		if len(runes) > 22 {
			name = string(runes[:22])
		}
		fmt.Fprintf(stdout, "\r\x1b[2K  %s Tests  %d/%d complete · %d running · %s  %.1fs", frames[frame%len(frames)], completed, len(selected), len(active), name, time.Since(started).Seconds())
		stdout.Flush()
		frame++
	}
	var progressStop chan struct{}
	var progressDone chan struct{}
	if human {
		title, reset := "", ""
		if o.color {
			title, reset = "\x1b[1;36m", "\x1b[0m"
		}
		outMu.Lock()
		fmt.Fprintf(stdout, "  %sflick test%s  %d tests\n\n", title, reset, len(selected))
		stdout.Flush()
		outMu.Unlock()
		started = time.Now()
	}
	if live {
		progressStop = make(chan struct{})
		progressDone = make(chan struct{})
		go func() {
			defer close(progressDone)
			ticker := time.NewTicker(80 * time.Millisecond)
			defer ticker.Stop()
			for {
				select {
				case <-ticker.C:
					outMu.Lock()
					draw()
					outMu.Unlock()
				case <-progressStop:
					return
				}
			}
		}()
	}
	for i, t := range selected {
		slots <- struct{}{}
		if fb.state.requested.Load() {
			break
		}
		wg.Add(1)
		go func() {
			defer func() { <-slots; wg.Done() }()
			var started time.Time
			if human {
				started = time.Now()
			}
			if live {
				outMu.Lock()
				active[i] = t.name
				draw()
				outMu.Unlock()
			}
			result := runOne(fb, t, seed, runs)
			results[i] = result
			if human {
				elapsed := time.Since(started)
				outMu.Lock()
				if live {
					delete(active, i)
					clear()
				}
				completed++
				writeTestResult(result, completed, len(selected), elapsed, o.color)
				draw()
				outMu.Unlock()
			}
		}()
	}
	wg.Wait()
	// an interrupted run still releases what its suites made
	for _, run := range runs {
		run.release(true)
	}
	if live {
		close(progressStop)
		<-progressDone
	}
	var elapsed time.Duration
	if human {
		elapsed = time.Since(started)
	}
	outMu.Lock()
	clear()
	code := 0
	if fb.state.requested.Load() {
		fmt.Fprintln(stdout, "interrupted: every running test closed its scope")
		code = 130
	} else if reportTests(results, skipped, seed, o.reporter, o.color, elapsed) > 0 {
		code = 1
	}
	stdout.Flush()
	outMu.Unlock()
	os.Exit(code)
}
