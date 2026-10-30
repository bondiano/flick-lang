package flickrt

import (
	"fmt"
	"time"
)

// Loggers, as far as `Logger.make`, the console and `Logger.layer` go: a
// layer installs loggers for its body, and what logs (the HTTP logger
// middleware) gives each installed one its message.
//
// shortcut: no logger is installed by default, where Effect's writes `timestamp=… level=… message=…` to the console, and `Effect.log` has no go form yet; port Effect's default logger with them

const loggersKey = "effect/Loggers"

// A logger turns a message's options into its output.
type logger struct {
	out func(fb *Fiber, options Value) Value
}

func LoggerMake(f Value) Value {
	return &logger{func(fb *Fiber, o Value) Value { return Apply(fb, f, o) }}
}

// LoggerWithConsoleLog writes each output as `console.log` does a string.
//
// shortcut: an output other than text is written by Go's `%v`, where Node inspects it; port Node's `inspect` when a program logs such outputs
func LoggerWithConsoleLog(l Value) Value {
	inner := l.(*logger)
	return &logger{func(fb *Fiber, o Value) Value {
		switch out := inner.out(fb, o).(type) {
		case string:
			StdoutLine(out)
		default:
			StdoutLine(fmt.Sprintf("%v", out))
		}
		return Unit
	}}
}

// LoggerLayer installs the loggers, beside the installed ones when the
// options merge them.
func LoggerLayer(options, loggers Value) Value {
	merge := record(options).Get("mergeWithExisting").(bool)
	return fromBuild(func(fb *Fiber, _ *MemoMap, _ *Owner) Services {
		installed := List(loggers)
		if existing, ok := fb.env.get(loggersKey); ok && merge {
			installed = append(append([]Value{}, List(existing)...), installed...)
		}
		return Services{loggersKey: installed}
	})
}

// logMessage gives each installed logger the message at the level.
//
// shortcut: a logger's `cause` is Unit, as RawCause has no go form yet
func logMessage(fb *Fiber, level, message string) {
	loggers, ok := fb.env.get(loggersKey)
	if !ok {
		return
	}
	options := Rec("message", []Value{message}, "logLevel", level, "cause", Unit,
		"date", float64(time.Now().UnixMilli()))
	for _, l := range List(loggers) {
		l.(*logger).out(fb, options)
	}
}
