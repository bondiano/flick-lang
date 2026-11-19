package flickrt

import (
	"strings"
	"sync"
)

// Effect's loggers: what `Effect.logWithLevel` gives each logger of the
// fiber, Effect's default ones (`defaultLogger` and `tracerLogger`) unless
// a layer or `with_loggers` installs others, from the minimum level on.

const (
	loggersKey     = "effect/Logger/CurrentLoggers"
	logToStderrKey = "effect/Logger/LogToStderr"
	// minLogLevelKey is where Effect's `MinimumLogLevel` is set for a task:
	// its value is the name of a level, as `"Info"`.
	minLogLevelKey = "effect/MinimumLogLevel"
)

// A logger turns a message into its output, in the fiber that logs it.
type logger struct {
	log func(fb *Fiber, e *logEntry) Value
}

// A logEntry is what Effect gives a logger: the message's parts, its
// level's name, its cause (empty for none) and its time in milliseconds.
type logEntry struct {
	message []Value
	level   string
	cause   *Cause
	date    float64
}

func loggerOf(v Value) *logger { return v.(*logger) }

// defaultLoggers are the loggers of a fiber nothing installed others in.
var defaultLoggers = []Value{defaultLogger, tracerLogger}

// currentLoggers are the fiber's loggers.
func currentLoggers(fb *Fiber) []Value {
	if ls, ok := fb.env.get(loggersKey); ok {
		return List(ls)
	}
	return defaultLoggers
}

// distinctLoggers are the loggers without repeats, as Effect's set of them.
func distinctLoggers(loggers []Value) []Value {
	out := make([]Value, 0, len(loggers))
	for _, l := range loggers {
		seen := false
		for _, o := range out {
			seen = seen || o == l
		}
		if !seen {
			out = append(out, l)
		}
	}
	return out
}

// logLevelOrder is Effect's rank of a level.
func logLevelOrder(level string) float64 {
	switch level {
	case "All":
		return -9007199254740991
	case "Fatal":
		return 50000
	case "Error":
		return 40000
	case "Warn":
		return 30000
	case "Info":
		return 20000
	case "Debug":
		return 10000
	case "Trace":
		return 0
	}
	return 9007199254740991
}

// minimumLogLevel is the fiber's minimum level, Effect's `Info` by default.
func minimumLogLevel(fb *Fiber) string {
	if l, ok := fb.env.get(minLogLevelKey); ok {
		return l.(string)
	}
	return "Info"
}

// withMinimumLogLevel runs the task logging only from the level on.
func withMinimumLogLevel(fb *Fiber, level string, task Value) Value {
	return withEnv(fb, minLogLevelKey, level, task)
}

// logMessage gives each logger of the fiber the message at the level, as
// `Effect.logWithLevel` does, unless the level is below the minimum.
func logMessage(fb *Fiber, level, message string) {
	logParts(fb, level, []Value{message}, emptyCause)
}

func logParts(fb *Fiber, level string, message []Value, cause *Cause) {
	if logLevelOrder(minimumLogLevel(fb)) > logLevelOrder(level) {
		return
	}
	loggers := currentLoggers(fb)
	if len(loggers) == 0 {
		return
	}
	e := &logEntry{message: message, level: level, cause: cause, date: clockOf(fb).millis()}
	for _, l := range loggers {
		loggerOf(l).log(fb, e)
	}
}

// logLevels are the `Log` service's levels as Effect's.
var logLevels = map[string]string{"debug": "Debug", "info": "Info", "warn": "Warn", "error": "Error"}

// LoggerWrite is the `write` of `Logger.effect_log`.
func LoggerWrite(fb *Fiber, level, message Value) Value {
	l, ok := logLevels[level.(string)]
	if !ok {
		l = "Info"
	}
	logMessage(fb, l, message.(string))
	return Unit
}

// options is the `Logger.Options` a Flick logger gets: the message's parts
// as text, a part other than text as Node inspects it.
func (e *logEntry) options() Value {
	return Rec("message", e.texts(), "logLevel", e.level, "cause", e.cause, "date", e.date)
}

func (e *logEntry) texts() []Value {
	out := make([]Value, len(e.message))
	for i, m := range e.message {
		out[i] = formatArg(m)
	}
	return out
}

func LoggerMake(f Value) Value {
	return &logger{func(_ *Fiber, e *logEntry) Value { return Apply(f, e.options()) }}
}

func LoggerMap(f, l Value) Value {
	inner := loggerOf(l)
	return &logger{func(fb *Fiber, e *logEntry) Value { return Apply(f, inner.log(fb, e)) }}
}

// writing is a logger writing the output of `l` by the console method of
// `write`, as Effect's `withConsoleLog` and the like.
func writing(l Value, write func(c *Console, level string, out Value)) Value {
	inner := loggerOf(l)
	return &logger{func(fb *Fiber, e *logEntry) Value {
		write(currentConsole(fb), e.level, inner.log(fb, e))
		return Unit
	}}
}

func LoggerWithConsoleLog(l Value) Value {
	return writing(l, func(c *Console, _ string, out Value) { c.log(formatArg(out)) })
}

func LoggerWithConsoleError(l Value) Value {
	return writing(l, func(c *Console, _ string, out Value) { c.error(formatArg(out)) })
}

// LoggerWithLeveledConsole writes by the level: debug and info as
// `console.log`, trace with its stack, warnings and errors to stderr.
func LoggerWithLeveledConsole(l Value) Value {
	return writing(l, func(c *Console, level string, out Value) {
		text := formatArg(out)
		switch level {
		case "Trace":
			stack := strings.TrimRight(flickFrames(), "\n")
			if stack != "" {
				stack = "\n" + stack
			}
			c.error("Trace: " + text + stack)
		case "Warn", "Error", "Fatal":
			c.error(text)
		default:
			c.log(text)
		}
	})
}

// LoggerLayer installs the loggers, beside the fiber's when the options
// merge them.
func LoggerLayer(options, loggers Value) Value {
	return loggersLayer(options, func(*Fiber, *Owner) []Value { return List(loggers) })
}

// loggersLayer installs the loggers `make` builds into the layer's scope.
func loggersLayer(options Value, make func(fb *Fiber, scope *Owner) []Value) Value {
	merge := record(options).Get("mergeWithExisting").(bool)
	return fromBuild(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		var installed []Value
		if merge {
			installed = append(installed, currentLoggers(fb)...)
		}
		installed = append(installed, make(fb, scope)...)
		return Services{loggersKey: distinctLoggers(installed)}
	})
}

func LoggerWithLoggers(fb *Fiber, loggers, task Value) Value {
	return withEnv(fb, loggersKey, distinctLoggers(List(loggers)), task)
}

func LoggerWithLogToStderr(fb *Fiber, toStderr, task Value) Value {
	return withEnv(fb, logToStderrKey, toStderr, task)
}

// logWriter is `console.error` under `with_log_to_stderr`, else `console.log`.
func logWriter(fb *Fiber) func(string) {
	c := currentConsole(fb)
	if v, ok := fb.env.get(logToStderrKey); ok && v.(bool) {
		return c.error
	}
	return c.log
}

// LoggerBatched installs a logger that gathers the outputs of `l` and
// passes them to `flush` each window, and what is left when the layer's
// scope closes.
func LoggerBatched(batch, options, l Value) Value {
	b := record(batch)
	return loggersLayer(options, func(fb *Fiber, scope *Owner) []Value {
		return []Value{batched(fb, scope, l, b.Get("window"), func(fb *Fiber, outputs []Value) {
			Run(fb, Apply(b.Get("flush"), outputs))
		})}
	})
}

// batched is Effect's `Logger.batched`: a fiber flushes the gathered
// outputs every window until the scope closes, which flushes the rest.
func batched(fb *Fiber, scope *Owner, l, window Value, flush func(*Fiber, []Value)) Value {
	inner := loggerOf(l)
	var (
		mu       sync.Mutex // guards the buffer
		flushing sync.Mutex // one flush at a time
		buffer   []Value
	)
	flushNow := func(fb *Fiber) {
		flushing.Lock()
		defer flushing.Unlock()
		mu.Lock()
		outputs := buffer
		buffer = nil
		mu.Unlock()
		if len(outputs) > 0 {
			Uninterruptible(fb, Eff(func(fb *Fiber) Value {
				flush(fb, outputs)
				return Unit
			}))
		}
	}
	daemon := *fb
	daemon.owner = newOwner()
	ticker := fork(&daemon, daemon.owner, Eff(func(fb *Fiber) Value {
		for {
			EffectSleep(fb, window)
			flushNow(fb)
		}
	}), nil)
	scope.add(fb, func(fb *Fiber, _ *Cause) Value {
		stop(fb, ticker)
		flushNow(fb)
		return Unit
	})
	return &logger{func(fb *Fiber, e *logEntry) Value {
		out := inner.log(fb, e)
		mu.Lock()
		buffer = append(buffer, out)
		mu.Unlock()
		return Unit
	}}
}

// LoggerToFile installs a logger appending the lines of `l` to the file
// at the path, opened through the fiber's `FileSystem` in the layer's
// scope, in batches of the window (a second by default).
func LoggerToFile(file, options, path, l Value) Value {
	f := record(file)
	flag, ok := some(f.Get("flag"))
	if !ok {
		flag = Nullary(Tag("FileSystem.OpenFlag.AppendRead"))
	}
	window, ok := some(f.Get("batchWindow"))
	if !ok {
		window = fromMillis(1000)
	}
	return loggersLayer(options, func(fb *Fiber, scope *Owner) []Value {
		impl, found := fb.env.get(fileSystemKey)
		if !found {
			panic(Defect{"Service not found: " + fileSystemKey})
		}
		open := Apply(Apply(record(impl).Get("open"), Rec("flag", Some(flag), "mode", f.Get("mode"), "noFollow", false)), path)
		handle := fileOf(Run(Within(fb, scope), open))
		return []Value{batched(fb, scope, l, window, func(fb *Fiber, outputs []Value) {
			lines := make([]string, len(outputs))
			for i, o := range outputs {
				lines[i] = o.(string)
			}
			// a failed write is ignored, as Effect's `toFile` ignores it
			catch(func() Value { return handle.writeAll([]byte(wellFormed(strings.Join(lines, "\n") + "\n"))) })
		})}
	})
}

// LogLevel's functions: Effect's levels by name, ranked.

var LogLevelValues Value = []Value{"All", "Fatal", "Error", "Warn", "Info", "Debug", "Trace", "None"}

func LogLevelGetOrdinal(level Value) Value { return logLevelOrder(level.(string)) }

func LogLevelIsGreaterThan(that, self Value) Value {
	return logLevelOrder(self.(string)) > logLevelOrder(that.(string))
}

func LogLevelIsGreaterThanOrEqualTo(that, self Value) Value {
	return logLevelOrder(self.(string)) >= logLevelOrder(that.(string))
}

func LogLevelIsLessThan(that, self Value) Value {
	return logLevelOrder(self.(string)) < logLevelOrder(that.(string))
}

func LogLevelIsLessThanOrEqualTo(that, self Value) Value {
	return logLevelOrder(self.(string)) <= logLevelOrder(that.(string))
}

// LogLevelIsEnabled tells whether the fiber logs at the level.
func LogLevelIsEnabled(fb *Fiber, level Value) Value {
	return logLevelOrder(minimumLogLevel(fb)) <= logLevelOrder(level.(string))
}
