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
	return d.Fields[0].(string), true
}

type named struct {
	name string
	test *test
	// the suites it is in, outermost first
	suites []*test
}

// flatten is the tests with their full names, `Module > group > name`.
func flatten(prefix string, tests []Value, out []named, suites []*test) []named {
	for _, v := range tests {
		t := v.(*test)
		name := prefix + " > " + t.name
		switch {
		case t.layer != nil:
			out = flatten(name, t.group, out, append(suites[:len(suites):len(suites)], t))
		case t.group != nil:
			out = flatten(name, t.group, out, suites)
		default:
			out = append(out, named{name, t, suites})
		}
	}
	return out
}

// suiteRun is a suite in a run: an owner of its own, into which its layer
// is built before its first selected test, and which closes after its last.
type suiteRun struct {
	once      sync.Once
	fiber     Fiber
	services  Services
	failure   *Cause
	mu        sync.Mutex
	remaining int
}

// suiteRuns is the run of each suite of the selected tests.
func suiteRuns(fb *Fiber, selected []named) map[*test]*suiteRun {
	runs := map[*test]*suiteRun{}
	for _, t := range selected {
		for _, s := range t.suites {
			if runs[s] == nil {
				owner := newOwner()
				owner.defects = &defects{}
				runs[s] = &suiteRun{fiber: Fiber{state: newState(), env: fb.env, owner: owner}}
			}
			runs[s].remaining++
		}
	}
	return runs
}

// enter is the fiber `inner` with the services of these suites, each built
// on first use; a failed build fails the test.
func enter(inner *Fiber, suites []*test, runs map[*test]*suiteRun) *Fiber {
	for _, s := range suites {
		run := runs[s]
		run.once.Do(func() {
			built, failure := catch(func() Value {
				return buildWith(&run.fiber, layerOf(s.layer), currentMemo(&run.fiber), run.fiber.owner)
			})
			if run.failure = failure; failure == nil {
				run.services = built.(Services)
			}
		})
		if run.failure != nil {
			panic(&Failure{run.failure})
		}
		inner = provided(inner, run.services)
	}
	return inner
}

// leave closes the suites this test ended; why one failed to close.
func leave(suites []*test, runs map[*test]*suiteRun) string {
	failed := ""
	for i := len(suites) - 1; i >= 0; i-- {
		if released := runs[suites[i]].release(false); released != nil {
			failed = describeReasons(released.reasons())
		}
	}
	return failed
}

// release closes the suite's owner after its last test, or at once when
// the run stopped early.
func (r *suiteRun) release(stopped bool) *Cause {
	r.mu.Lock()
	if !stopped {
		r.remaining--
	}
	last := r.remaining == 0 || stopped && r.remaining > 0
	if stopped {
		r.remaining = 0
	}
	r.mu.Unlock()
	if !last {
		return nil
	}
	return r.fiber.owner.closeFinalizers(&r.fiber, nil)
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
	switch x := v.(type) {
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
			return previewObject(append([]string{"_id", "_tag"}, fieldNames(x)...), append([]Value{"Option", x.Tag}, x.Fields...)), true
		case "Success":
			return previewObject([]string{"_id", "_tag", "value"}, []Value{"Result", x.Tag, x.Fields[0]}), true
		case "Failure":
			return previewObject([]string{"_id", "_tag", "failure"}, []Value{"Result", x.Tag, x.Fields[0]}), true
		}
		return previewObject(append([]string{"_tag"}, fieldNames(x)...), append([]Value{x.Tag}, x.Fields...)), true
	}
	return Format(v), true
}

// fieldNames is the JS property of each field of a variant value.
func fieldNames(d *Data) []string {
	out := make([]string, len(d.Fields))
	for i := range d.Fields {
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
	case *Data:
		return ConstructorName(x.Tag)
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
			return "assertion", d.Fields[0].(string)
		}
	}
	return "failure", describeReasons(cause.reasons())
}

// runOne runs a test in a fiber and an owner of its own.
func runOne(fb *Fiber, t named, seed int64, runs map[*test]*suiteRun) testResult {
	result := runLeaf(fb, t, seed, runs)
	if len(t.suites) == 0 {
		return result
	}
	// the suite's release fails the test that ended it
	if failed := leave(t.suites, runs); failed != "" {
		if result.Status == "passed" {
			result.Kind, result.Message = "cleanup", failed
		}
		result.Status, result.Cleanup = "failed", failed
	}
	return result
}

func runLeaf(fb *Fiber, t named, seed int64, runs map[*test]*suiteRun) testResult {
	owner := newOwner()
	owner.defects = &defects{}
	inner := Fiber{state: newState(), env: fb.env, owner: owner}
	// stopping the run interrupts each running test
	done := make(chan struct{})
	defer close(done)
	go func() {
		select {
		case <-fb.state.signal:
			inner.state.interrupt(fb.state.id)
		case <-done:
		}
	}()
	var message string
	var bad bool
	_, exit := catch(func() Value {
		if t.test.task != nil {
			return Run(enter(&inner, t.suites, runs), t.test.task)
		}
		message, bad = t.test.checks(fb, seed)
		return Unit
	})
	released := owner.closeFinalizers(&inner, exit)
	// a supervisor that failed during the test fails it, as it fails a program
	exit = Sequential(exit, owner.defects.all())
	kind, text := outcome(message, bad, exit)
	cleanup := ""
	if released != nil {
		cleanup = describeReasons(released.reasons())
	}
	switch {
	case kind == "" && cleanup == "":
		return testResult{Name: t.name, Status: "passed"}
	case kind == "":
		kind, text = "cleanup", cleanup
	}
	return testResult{Name: t.name, Status: "failed", Kind: kind, Message: text, Cleanup: cleanup}
}

// testOptions are the options of `flick test`, from the runner's arguments.
type testOptions struct {
	filter, reporter string
	jobs             int
	seed             int64
	seeded, watched  bool
	color, progress  bool
}

func parseTestArgs(args []string) testOptions {
	o := testOptions{reporter: "human", jobs: 1}
	for i := 0; i < len(args); i++ {
		value := ""
		if i+1 < len(args) {
			value = args[i+1]
		}
		switch args[i] {
		case "--filter":
			o.filter = value
		case "--jobs":
			if n, err := strconv.Atoi(value); err == nil && n > 0 {
				o.jobs = n
			}
		case "--seed":
			if n, err := strconv.ParseInt(value, 10, 64); err == nil {
				o.seed, o.seeded = n, true
			}
		case "--reporter":
			o.reporter = value
		case "--color":
			o.color = value == "always"
		case "--progress":
			o.progress = value == "live"
		case "--watched":
			o.watched = true
			continue
		default:
			continue
		}
		i++
	}
	return o
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

func reportTests(results []testResult, seed int64, reporter string, color bool, elapsed time.Duration) int {
	failed := 0
	for _, r := range results {
		if r.Status == "failed" {
			failed++
		}
	}
	passed := len(results) - failed
	if reporter == "json" {
		var tests []string
		for _, r := range results {
			tests = append(tests, jsonOf(r))
		}
		fmt.Fprintf(stdout, "{\"seed\":%d,\"passed\":%d,\"failed\":%d,\"tests\":[%s]}\n", seed, passed, failed, strings.Join(tests, ","))
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
	fmt.Fprintf(stdout, "\n  Tests  %s%d%s passed · %s%d%s failed · %d total\n", passColor, passed, reset, failColor, failed, reset, len(results))
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
	var selected []named
	for _, s := range suites {
		for _, t := range flatten(s.Module, List(s.Tests), nil, nil) {
			if strings.Contains(t.name, o.filter) {
				selected = append(selected, t)
			}
		}
	}
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
	} else if reportTests(results, seed, o.reporter, o.color, elapsed) > 0 {
		code = 1
	}
	stdout.Flush()
	outMu.Unlock()
	os.Exit(code)
}
