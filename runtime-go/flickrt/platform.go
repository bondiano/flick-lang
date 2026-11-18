package flickrt

import (
	"bufio"
	"fmt"
	"os"
	"os/signal"
	"runtime"
	"runtime/debug"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
)

var (
	outMu  sync.Mutex // fibers write from many goroutines
	stdout = bufio.NewWriterSize(os.Stdout, 64<<10)
)

// idle writes out what stdout holds before a fiber blocks, so what a
// program printed shows while it waits, as on Node.
func idle() {
	outMu.Lock()
	if stdout.Buffered() > 0 {
		stdout.Flush()
	}
	outMu.Unlock()
}

// StdoutLine writes a line to stdout.
func StdoutLine(text Value) Value { return StdoutWrite(text.(string) + "\n") }

// StdoutWrite writes text to stdout without a line break.
func StdoutWrite(text Value) Value {
	t := wellFormed(text.(string))
	outMu.Lock()
	stdout.WriteString(t)
	// a long-running program's lines show as they come once fibers run beside main
	if concurrent.Load() && strings.HasSuffix(t, "\n") {
		stdout.Flush()
	}
	outMu.Unlock()
	return Unit
}

// StderrLine writes a line to stderr, after what stdout holds so far.
func StderrLine(text Value) Value {
	outMu.Lock()
	stdout.Flush()
	os.Stderr.WriteString(wellFormed(text.(string)) + "\n")
	outMu.Unlock()
	return Unit
}

// PlatformArgs is the arguments of the program.
func PlatformArgs() Value {
	out := make([]Value, len(os.Args)-1)
	for i, a := range os.Args[1:] {
		out[i] = a
	}
	return out
}

// PlatformEnv is the environment variable `name`, if set.
func PlatformEnv(name Value) Value {
	if v, ok := os.LookupEnv(name.(string)); ok {
		return Some(v)
	}
	return None
}

// PlatformVars is every environment variable, as pairs of its name and
// value.
func PlatformVars() Value {
	env := os.Environ()
	out := make([]Value, 0, len(env))
	for _, kv := range env {
		if k, v, ok := strings.Cut(kv, "="); ok {
			out = append(out, []Value{k, v})
		}
	}
	return out
}

// hostEnvironment is the fiber's `Host.Environment`: what a std function
// reads of the environment by default goes through it, so a fake one
// answers.
func hostEnvironment(fb *Fiber) *Record {
	impl, ok := fb.env.get("Host.Environment")
	if !ok {
		panic(Defect{"Service not found: Host.Environment"})
	}
	return record(impl)
}

// hostEnv is `Host.Environment.env name`, "" when the variable is unset.
func hostEnv(fb *Fiber, name string) string {
	if v := Run(fb, Apply(fb, hostEnvironment(fb).Get("env"), name)); isSome(v) {
		return value(v).(string)
	}
	return ""
}

// hostVars is `Host.Environment.vars ()`.
func hostVars(fb *Fiber) Value { return Run(fb, Apply(fb, hostEnvironment(fb).Get("vars"), Unit)) }

// Todo is `todo`: code not written yet fails when it runs.
func Todo(message Value) Value { panic(Defect{"Error: " + message.(string)}) }

// Assert is a failing `let assert`: its place and message, and the value
// it missed.
func Assert(message, value Value) Value {
	panic(Defect{fmt.Sprintf("Error: %s: %s", message.(string), Format(value))})
}

// Echo is `echo`: shows a value and where it was echoed, and returns it.
func Echo(location, value Value) Value {
	StderrLine(location.(string) + " " + Format(value))
	return value
}

// Format shows any value for debugging.
func Format(v Value) string {
	switch x := asData(v).(type) {
	case []Value:
		out := "["
		for i, e := range x {
			if i > 0 {
				out += ","
			}
			out += Format(e)
		}
		return out + "]"
	case *Data:
		out := "{\"_tag\":" + QuoteJSON(x.Tag)
		for i, f := range x.Fields() {
			out += fmt.Sprintf(",%q:%s", nativeField(x.Tag, i), Format(f))
		}
		return out + "}"
	case *Record:
		out := "{"
		for i, k := range x.keys {
			if i > 0 {
				out += ","
			}
			out += QuoteJSON(k) + ":" + Format(x.vals[i])
		}
		return out + "}"
	}
	return Show(v).(string)
}

func liveHostEnv(e *env) *env {
	consoleRec := Rec(
		"stdout_line", Fn(func(_ *Fiber, text Value) Value {
			return Eff(func(_ *Fiber) Value { return StdoutLine(text) })
		}),
		"stderr_line", Fn(func(_ *Fiber, text Value) Value {
			return Eff(func(_ *Fiber) Value { return StderrLine(text) })
		}),
		"stdout_write", Fn(func(_ *Fiber, text Value) Value {
			return Eff(func(_ *Fiber) Value { return StdoutWrite(text) })
		}),
	)
	clockRec := Rec(
		"current_time_millis", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(fb *Fiber) Value { return ClockCurrentTimeMillis(fb) })
		}),
		"current_time_nanos", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(fb *Fiber) Value { return ClockCurrentTimeNanos(fb) })
		}),
		"monotonic_time_nanos", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(fb *Fiber) Value { return ClockMonotonicTimeNanos(fb) })
		}),
	)
	randomRec := Rec(
		"next", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(fb *Fiber) Value { return RandomNext(fb) })
		}),
	)
	envRec := Rec(
		"env", Fn(func(_ *Fiber, name Value) Value {
			return Eff(func(_ *Fiber) Value { return PlatformEnv(name) })
		}),
		"vars", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(_ *Fiber) Value { return PlatformVars() })
		}),
	)
	argsRec := Rec(
		"args", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(_ *Fiber) Value { return PlatformArgs() })
		}),
	)
	processRec := Rec(
		"spawn", Fn(func(_ *Fiber, command Value) Value {
			return Eff(func(fb *Fiber) Value { return ChildProcessSpawn(fb, command) })
		}),
	)
	return e.
		withHandler("Host.Console", record(consoleRec)).
		withHandler("Host.Clock", record(clockRec)).
		withHandler("Host.Random", record(randomRec)).
		withHandler("Host.Environment", record(envRec)).
		withHandler("Host.Arguments", record(argsRec)).
		withHandler("Host.Process", record(processRec))
}

// Main runs the program: `run` starts `main` in the root fiber, under the
// root owner. SIGINT and SIGTERM interrupt it, and the owner closes as
// after any end. The exit code is the JS runner's.
func Main(run func(*Fiber)) {
	// a Flick program allocates many small values: collecting at 4x the
	// live heap instead of 2x costs megabytes and saves a third of the CPU
	// a busy server spends; GOGC set by the user wins
	if _, set := os.LookupEnv("GOGC"); !set {
		debug.SetGCPercent(400)
	}
	fb := rootFiber()
	fb.env = liveHostEnv(fb.env)
	var stopped atomic.Value // the signal that interrupted main
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM)
	go func() {
		s := <-signals
		stopped.Store(s)
		fb.state.interrupt(0)
	}()
	_, cause := catch(func() Value {
		return region(fb, func(fb *Fiber, owner *Owner) Value {
			if tracing {
				traceOpen(owner, nil, "main")
			}
			run(fb)
			return Unit
		})
	})
	s, _ := stopped.Load().(os.Signal)
	rootDefects.mu.Lock()
	if tracing {
		traceEvent("exit", "x", traceExit(cause))
	}
	code := conclude(cause, s, rootDefects.causes)
	rootDefects.mu.Unlock()
	stdout.Flush()
	os.Exit(code)
}

// conclude reports how main ended, as the JS runner's `conclude`: the
// failures of supervisors, then main's; 130 or 143 when only a signal
// stopped it.
func conclude(cause *Cause, s os.Signal, supervisors []*Cause) int {
	write := func(text string) {
		stdout.Flush()
		os.Stderr.WriteString(text)
	}
	for _, c := range supervisors {
		write("error: a supervisor failed\n" + pretty(c) + "\n")
	}
	if cause == nil {
		if len(supervisors) > 0 {
			return 1
		}
		return 0
	}
	if cause.has(causeInterrupt) && !cause.has(causeFail) && !cause.has(causeDie) {
		if s != nil && len(supervisors) == 0 {
			if s == os.Interrupt {
				return 130
			}
			return 143
		}
		if s == nil {
			write("error: main was interrupted\n")
		}
		return 1
	}
	write(report(cause) + "\n")
	return 1
}

// flickFrames is the Flick source lines of the stack a defect unwinds, as
// the `//line` directives of the generated code name them; called while
// the panic's frames are still on the stack.
func flickFrames() string {
	pcs := make([]uintptr, 256)
	frames := runtime.CallersFrames(pcs[:runtime.Callers(2, pcs)])
	var out strings.Builder
	last := ""
	for {
		frame, more := frames.Next()
		if strings.HasSuffix(frame.File, ".fl") {
			at := fmt.Sprintf("    at %s:%d\n", frame.File, frame.Line)
			if at != last {
				out.WriteString(at)
				last = at
			}
		}
		if !more {
			return out.String()
		}
	}
}
