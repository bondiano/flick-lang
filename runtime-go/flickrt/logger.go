package flickrt

import (
	"strings"
	"sync"
)

// Effect's loggers: a logger turns each message a fiber logs into its
// output, which the console loggers write. The fiber carries its loggers
// (Effect's default and tracer loggers unless a layer or `with_loggers`
// sets others), its log levels, annotations and log spans.

const (
	loggersKey           = "effect/Loggers"
	logToStderrKey       = "effect/Logger/LogToStderr"
	currentLogLevelKey   = "effect/References/CurrentLogLevel"
	logAnnotationsKey    = "effect/References/CurrentLogAnnotations"
	logSpansKey          = "effect/References/CurrentLogSpans"
	unhandledLogLevelKey = "effect/References/UnhandledLogLevel"
)

// A logger turns a message into its output.
type logger struct {
	log func(fb *Fiber, m *logEntry) Value
}

// logEntry is Effect's `Logger.Options`: the parts of a message, its
// level, the cause logged with it (nil for none) and its time in
// milliseconds since the epoch.
type logEntry struct {
	message []Value
	level   string
	cause   *Cause
	date    float64
}

func loggerOf(v Value) *logger { return v.(*logger) }

// defaultLoggers are Effect's CurrentLoggers by default.
var defaultLoggers = []Value{LoggerDefault, LoggerTracer}

// loggersOf is the fiber's loggers.
func loggersOf(fb *Fiber) []Value {
	if ls, ok := fb.env.get(loggersKey); ok {
		return List(ls)
	}
	return defaultLoggers
}

// loggerSet is Effect's Set of loggers: `existing`, then each of `add`
// it lacks.
func loggerSet(existing, add []Value) []Value {
	out := append([]Value{}, existing...)
	for _, l := range add {
		found := false
		for _, e := range out {
			found = found || e == l
		}
		if !found {
			out = append(out, l)
		}
	}
	return out
}

// messageParts is the parts of a message as text: Node's inspection of
// one that is not a string.
func messageParts(message []Value) []Value {
	out := make([]Value, len(message))
	for i, part := range message {
		out[i] = attrText(part)
	}
	return out
}

// LoggerMake is `Logger.make`: `f` gets the options as a Flick record,
// whose `cause` is a RawCause, empty without one.
func LoggerMake(f Value) Value {
	return &logger{func(fb *Fiber, m *logEntry) Value {
		return Apply(fb, f, Rec("message", messageParts(m.message), "logLevel", m.level, "cause", m.cause, "date", m.date))
	}}
}

func LoggerMap(f, l Value) Value {
	inner := loggerOf(l)
	return &logger{func(fb *Fiber, m *logEntry) Value { return Apply(fb, f, inner.log(fb, m)) }}
}

// consoleText is what a Node console writes of its arguments: strings as
// they are, other values inspected, separated by spaces.
func consoleText(args ...Value) string {
	parts := make([]string, len(args))
	for i, a := range args {
		parts[i] = attrText(a)
	}
	return strings.Join(parts, " ")
}

// withConsole writes each output of `l` to the fiber's console as
// `write` does at the message's level.
func withConsole(l Value, write func(c *console, level, text string)) Value {
	inner := loggerOf(l)
	return &logger{func(fb *Fiber, m *logEntry) Value {
		write(currentConsole(fb), m.level, consoleText(inner.log(fb, m)))
		return Unit
	}}
}

func LoggerWithConsoleLog(l Value) Value {
	return withConsole(l, func(c *console, _, text string) { c.log(text) })
}

func LoggerWithConsoleError(l Value) Value {
	return withConsole(l, func(c *console, _, text string) { c.warn(text) })
}

// LoggerWithLeveledConsole writes with the console method of the level.
// shortcut: no stack follows `Trace: …`, as with `Console.trace`
func LoggerWithLeveledConsole(l Value) Value {
	return withConsole(l, func(c *console, level, text string) {
		switch level {
		case "Trace":
			c.warn("Trace: " + text)
		case "Warn", "Error", "Fatal":
			c.warn(text)
		default:
			c.log(text)
		}
	})
}

// logWith gives each of the fiber's loggers the message at the level,
// unless the fiber's minimum level is above it.
func logWith(fb *Fiber, level string, message []Value, cause *Cause) {
	if logLevelOrdinal(minimumLogLevel(fb)) > logLevelOrdinal(level) {
		return
	}
	loggers := loggersOf(fb)
	if len(loggers) == 0 {
		return
	}
	m := &logEntry{message, level, cause, clockOf(fb).millis()}
	for _, l := range loggers {
		loggerOf(l).log(fb, m)
	}
}

// logMessage logs a message of the runtime's at the level.
func logMessage(fb *Fiber, level, message string) { logWith(fb, level, []Value{message}, nil) }

func currentLogLevel(fb *Fiber) string { return envOr(fb, currentLogLevelKey, "Info").(string) }

// logAt logs the message at the level.
func logAt(fb *Fiber, level string, message Value) Value {
	logWith(fb, level, []Value{message}, nil)
	return Unit
}

// EffectLog is `Effect.log`, at the fiber's current level.
func EffectLog(fb *Fiber, message Value) Value        { return logAt(fb, currentLogLevel(fb), message) }
func EffectLogFatal(fb *Fiber, message Value) Value   { return logAt(fb, "Fatal", message) }
func EffectLogError(fb *Fiber, message Value) Value   { return logAt(fb, "Error", message) }
func EffectLogWarning(fb *Fiber, message Value) Value { return logAt(fb, "Warn", message) }
func EffectLogInfo(fb *Fiber, message Value) Value    { return logAt(fb, "Info", message) }
func EffectLogDebug(fb *Fiber, message Value) Value   { return logAt(fb, "Debug", message) }
func EffectLogTrace(fb *Fiber, message Value) Value   { return logAt(fb, "Trace", message) }

func EffectLogWithLevel(fb *Fiber, level, message Value) Value {
	return logAt(fb, level.(string), message)
}

var logLevels = map[string]string{"debug": "Debug", "info": "Info", "warn": "Warn", "error": "Error"}

// LoggerLogWrite is the `write` of `Logger.effect_log`: the `Log` level
// of the message as Effect's.
func LoggerLogWrite(fb *Fiber, level, message Value) Value {
	l, ok := logLevels[level.(string)]
	if !ok {
		l = "Info"
	}
	return logAt(fb, l, message)
}

// loggerLayer installs the logger `build` makes into the layer's scope,
// beside the installed ones when the options merge them.
func loggerLayer(options Value, build func(fb *Fiber, scope *Owner) []Value) Value {
	merge := record(options).Get("mergeWithExisting").(bool)
	return fromBuild(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		var existing []Value
		if merge {
			existing = loggersOf(fb)
		}
		return Services{loggersKey: loggerSet(existing, build(fb, scope))}
	})
}

func LoggerLayer(options, loggers Value) Value {
	return loggerLayer(options, func(*Fiber, *Owner) []Value { return List(loggers) })
}

// LoggerWithLoggers is `Logger.with_loggers`: the task runs with exactly
// these loggers.
func LoggerWithLoggers(fb *Fiber, loggers, task Value) Value {
	return withEnv(fb, loggersKey, loggerSet(nil, List(loggers)), task)
}

func LoggerWithLogToStderr(fb *Fiber, toStderr, task Value) Value {
	return withEnv(fb, logToStderrKey, toStderr, task)
}

// batched is Effect's `Logger.batched`: the outputs of `inner` gather in
// a buffer that a detached fiber flushes every window, and that `scope`
// flushes as it closes, once that fiber is interrupted.
func batched(fb *Fiber, scope *Owner, inner *logger, window *Duration, flush func(*Fiber, []Value)) Value {
	var mu sync.Mutex
	var buffer []Value
	drain := func(fb *Fiber) Value {
		return masked(fb, fb.state.mask+1, func() Value {
			mu.Lock()
			out := buffer
			buffer = nil
			mu.Unlock()
			if len(out) > 0 {
				flush(fb, out)
			}
			return Unit
		})
	}
	flusher := fork(fb, newOwner(), Eff(func(fb *Fiber) Value {
		for {
			sleepFor(fb, window)
			drain(fb)
		}
	}), nil)
	scope.add(fb, func(fb *Fiber, _ *Cause) Value {
		stop(fb, flusher)
		return drain(fb)
	})
	return &logger{func(fb *Fiber, m *logEntry) Value {
		out := inner.log(fb, m)
		mu.Lock()
		buffer = append(buffer, out)
		mu.Unlock()
		return Unit
	}}
}

// LoggerBatched is `Logger.batched`, installed by a layer that owns its scope.
func LoggerBatched(batch, options, l Value) Value {
	b := record(batch)
	window, flush := dur(b.Get("window")), b.Get("flush")
	return loggerLayer(options, func(fb *Fiber, scope *Owner) []Value {
		return []Value{batched(fb, scope, loggerOf(l), window, func(fb *Fiber, out []Value) {
			Run(fb, Apply(fb, flush, out))
		})}
	})
}

var defaultLogFileWindow = &Duration{tag: durationMillis, millis: 1000}

// LoggerToFile is `Logger.to_file`: the lines of `l` appended to the file,
// which the fiber's FileSystem service opens into the layer's scope,
// a batch at a time.
func LoggerToFile(file, options, path, l Value) Value {
	f := record(file)
	window := defaultLogFileWindow
	if w, ok := some(f.Get("batchWindow")); ok {
		window = dur(w)
	}
	flag := f.Get("flag")
	if !isSome(flag) {
		flag = Some(Nullary(Tag("FileSystem.OpenFlag.AppendRead")))
	}
	open := Rec("flag", flag, "mode", f.Get("mode"), "noFollow", false)
	return loggerLayer(options, func(fb *Fiber, scope *Owner) []Value {
		fs, ok := fb.env.get("FileSystem.FileSystem")
		if !ok {
			panic(Defect{"Service not found: FileSystem.FileSystem"})
		}
		opened := Run(Within(fb, scope), Apply(fb, Apply(fb, fs.(*Record).Get("open"), open), path))
		return []Value{batched(fb, scope, loggerOf(l), window, func(fb *Fiber, out []Value) {
			lines := make([]string, len(out))
			for i, line := range out {
				lines[i] = line.(string)
			}
			// Effect ignores a failed write
			catch(func() Value {
				file := fsFileOf(opened)
				file.mu.Lock()
				defer file.mu.Unlock()
				return file.write([]byte(strings.Join(lines, "\n")+"\n"), "writeAll")
			})
		})}
	})
}
