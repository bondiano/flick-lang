package flickrt

import (
	"os"
	"strconv"
	"strings"
	"time"
	"unicode"
)

// Effect's built-in loggers: the formats, the pretty console loggers and
// the default one, as Effect writes them.

const (
	// logAnnotationsKey holds the fiber's log annotations: a record.
	logAnnotationsKey = "effect/LogAnnotations"
	// logSpansKey holds the fiber's log spans: [label, start millis] pairs,
	// innermost first.
	logSpansKey = "effect/LogSpans"
)

func logAnnotations(fb *Fiber) *Record {
	if a, ok := fb.env.get(logAnnotationsKey); ok {
		return a.(*Record)
	}
	return &Record{}
}

func logSpans(fb *Fiber) []Value {
	if s, ok := fb.env.get(logSpansKey); ok {
		return List(s)
	}
	return nil
}

// formatLabel is a label with its spaces, `=` and `"` made `_`.
func formatLabel(key string) string {
	return strings.Map(func(r rune) rune {
		if r == '=' || r == '"' || unicode.IsSpace(r) || r == '\ufeff' {
			return '_'
		}
		return r
	}, key)
}

// spanTexts are the log spans as `label=<elapsed>ms`.
func spanTexts(fb *Fiber, now float64) []string {
	var out []string
	for _, s := range logSpans(fb) {
		span := List(s)
		out = append(out, formatLabel(span[0].(string))+"="+ShowNumber(now-span[1].(float64))+"ms")
	}
	return out
}

// isoTimestamp is a time in milliseconds as `Date.toISOString` writes it.
func isoTimestamp(ms float64) string { return (&date{timeClip(ms)}).isoString() }

// clockTime is Effect's default date format: the local time of day.
func clockTime(ms float64) string {
	t := time.UnixMilli(int64(ms)).Local()
	return t.Format("15:04:05.") + strconv.Itoa(t.Nanosecond()/1e6+1000)[1:]
}

func fiberLabel(fb *Fiber) string { return "#" + strconv.FormatInt(fb.state.id, 10) }

// formatUnknown is a message part or annotation in a line of text.
//
// shortcut: a value other than text is its JSON, where Effect's `Formatter.format` writes class names and `n` suffixes; port it when loggers get such values
func formatUnknown(v Value) string {
	if s, ok := v.(string); ok {
		return s
	}
	return JsonStringify(v).(string)
}

// textOnly tells whether a value needs no quotes in a line.
func textOnly(s string) bool {
	return !strings.ContainsFunc(s, func(r rune) bool {
		return r == '"' || r == '=' || unicode.IsSpace(r) || r == '\ufeff'
	})
}

// escapeDoubleQuotes quotes a value as `format_simple` does: quotes not
// already escaped get a backslash.
func escapeDoubleQuotes(s string) string {
	var out strings.Builder
	out.WriteByte('"')
	for i := 0; i < len(s); i++ {
		switch {
		case s[i] == '\\' && i+1 < len(s):
			out.WriteByte('\\')
			i++
			out.WriteByte(s[i])
		case s[i] == '"':
			out.WriteString(`\"`)
		default:
			out.WriteByte(s[i])
		}
	}
	out.WriteByte('"')
	return out.String()
}

// formatLine is Effect's `key=value` line of `format_simple` and
// `format_logfmt`, which quote values by `quote`.
func formatLine(quote func(string) string) Value {
	return &logger{func(fb *Fiber, e *logEntry) Value {
		value := func(v string) string {
			if textOnly(v) {
				return v
			}
			return quote(v)
		}
		out := "timestamp=" + value(isoTimestamp(e.date)) + " level=" + value(strings.ToUpper(e.level)) +
			" fiber=" + value(fiberLabel(fb))
		for _, m := range e.message {
			out += " message=" + value(formatUnknown(m))
		}
		if len(e.cause.reasons()) > 0 {
			out += " cause=" + value(pretty(e.cause))
		}
		for _, s := range spanTexts(fb, e.date) {
			out += " " + s
		}
		a := logAnnotations(fb)
		for i, k := range a.keys {
			out += " " + formatLabel(k) + "=" + value(formatUnknown(a.vals[i]))
		}
		return out
	}}
}

var (
	LoggerFormatSimple Value = formatLine(escapeDoubleQuotes)
	LoggerFormatLogFmt Value = formatLine(QuoteJSON)
)

// A structured message, as Effect's `formatStructured` makes it, and the
// parts of it the Flick record `Logger.Structured` has as text.
type structured struct {
	message            Value // a part, or the parts when there are several
	level, timestamp   string
	cause              Value // the pretty cause, or Unit for none
	annotations, spans *Record
	fiberId            string
	parts              []Value
	annotationTexts    []Value
	spanEntries        []Value
	causeText          Value // an Option of the pretty cause
}

func structure(fb *Fiber, e *logEntry) *structured {
	s := &structured{
		level: strings.ToUpper(e.level), timestamp: isoTimestamp(e.date), cause: Unit, causeText: None,
		fiberId: fiberLabel(fb), parts: e.texts(),
	}
	s.message = e.message
	if len(e.message) == 1 {
		s.message = e.message[0]
	}
	if len(e.cause.reasons()) > 0 {
		text := pretty(e.cause)
		s.cause, s.causeText = text, Some(text)
	}
	a := logAnnotations(fb)
	s.annotations = a
	s.annotationTexts = []Value{}
	for i, k := range a.keys {
		s.annotationTexts = append(s.annotationTexts, []Value{k, formatArg(a.vals[i])})
	}
	spans := newRecord(0)
	s.spanEntries = []Value{}
	for _, sp := range logSpans(fb) {
		span := List(sp)
		elapsed := e.date - span[1].(float64)
		spans.put(span[0].(string), elapsed)
		s.spanEntries = append(s.spanEntries, []Value{span[0], elapsed})
	}
	s.spans = spans
	return s
}

// effectObject is the structured message as Effect's object, which the
// console inspects.
func (s *structured) effectObject() Value {
	return Rec("message", s.message, "level", s.level, "timestamp", s.timestamp, "cause", s.cause,
		"annotations", s.annotations, "spans", s.spans, "fiberId", s.fiberId)
}

// json is the structured message as `Formatter.formatJson` writes it.
func (s *structured) json() string {
	out := `{"message":` + JsonStringify(s.message).(string) + `,"level":` + QuoteJSON(s.level) +
		`,"timestamp":` + QuoteJSON(s.timestamp)
	if text, ok := s.cause.(string); ok {
		out += `,"cause":` + QuoteJSON(text)
	}
	return out + `,"annotations":` + JsonStringify(s.annotations).(string) + `,"spans":` +
		JsonStringify(s.spans).(string) + `,"fiberId":` + QuoteJSON(s.fiberId) + "}"
}

var (
	LoggerFormatStructured Value = &logger{func(fb *Fiber, e *logEntry) Value {
		s := structure(fb, e)
		return Rec("level", s.level, "fiberId", s.fiberId, "timestamp", s.timestamp, "message", s.parts,
			"cause", s.causeText, "annotations", s.annotationTexts, "spans", s.spanEntries)
	}}
	LoggerFormatJson Value = &logger{func(fb *Fiber, e *logEntry) Value { return structure(fb, e).json() }}

	LoggerConsoleLogFmt     Value = LoggerWithConsoleLog(LoggerFormatLogFmt)
	LoggerConsoleJson       Value = LoggerWithConsoleLog(LoggerFormatJson)
	LoggerConsoleStructured Value = &logger{func(fb *Fiber, e *logEntry) Value {
		currentConsole(fb).log(inspectCustom(structure(fb, e).effectObject()))
		return Unit
	}}
	LoggerDefault Value = defaultLogger
	LoggerTracer  Value = tracerLogger
)

// defaultLogger is Effect's: `[time] LEVEL (#fiber): message` on the
// console, the cause and annotations after the message.
var defaultLogger Value = &logger{func(fb *Fiber, e *logEntry) Value {
	parts := []string{"[" + clockTime(e.date) + "] " + strings.ToUpper(e.level) + " (" + fiberLabel(fb) + ")" +
		joinSpans(spanTexts(fb, e.date)) + ":"}
	for _, m := range e.message {
		parts = append(parts, formatArg(m))
	}
	if len(e.cause.reasons()) > 0 {
		parts = append(parts, pretty(e.cause))
	}
	if a := logAnnotations(fb); len(a.keys) > 0 {
		parts = append(parts, inspectCustom(a))
	}
	logWriter(fb)(strings.Join(parts, " "))
	return Unit
}}

func joinSpans(spans []string) string {
	out := ""
	for _, s := range spans {
		out += " " + s
	}
	return out
}

// tracerLogger is Effect's, which adds each message as an event of the
// fiber's span.
//
// shortcut: spans keep no events on the go target, so this logger writes nothing; record the event when spans are exported
var tracerLogger Value = &logger{func(*Fiber, *logEntry) Value { return Unit }}

var levelColors = map[string][]string{
	"Trace": {"90"}, "Debug": {"34"}, "Info": {"32"}, "Warn": {"33"}, "Error": {"31"}, "Fatal": {"101", "30"},
}

// prettyOptions are the options of the pretty loggers as Go values.
type prettyOptions struct {
	colors     bool
	formatDate func(ms float64) string
}

func prettyOf(colors bool, formatDate Value) prettyOptions {
	o := prettyOptions{colors: colors, formatDate: clockTime}
	if f, ok := some(formatDate); ok {
		o.formatDate = func(ms float64) string { return Apply(f, ms).(string) }
	}
	return o
}

// prettyTty is Effect's `consolePrettyTty`.
func prettyTty(o prettyOptions) Value {
	color := func(text string, codes ...string) string {
		if !o.colors {
			return text
		}
		out := ""
		for _, c := range codes {
			out += "\x1b[" + c + "m"
		}
		return out + text + "\x1b[0m"
	}
	return &logger{func(fb *Fiber, e *logEntry) Value {
		c := currentConsole(fb)
		log := logWriter(fb)
		first := color("["+o.formatDate(e.date)+"]", "37") + " " + color(strings.ToUpper(e.level), levelColors[e.level]...) +
			" (" + fiberLabel(fb) + ")" + joinSpans(spanTexts(fb, e.date)) + ":"
		rest := e.message
		if len(rest) > 0 {
			if s, ok := rest[0].(string); ok {
				first += " " + color(s, "1", "36")
				rest = rest[1:]
			}
		}
		log(first)
		c.indentGroup()
		defer c.groupEnd()
		if len(e.cause.reasons()) > 0 {
			log(pretty(e.cause))
		}
		for _, m := range rest {
			log(formatArg(m))
		}
		a := logAnnotations(fb)
		for i, k := range a.keys {
			log(color(k+":", "1", "37") + " " + formatArg(a.vals[i]))
		}
		return Unit
	}}
}

// prettyBrowser is Effect's `consolePrettyBrowser`: its CSS styles, which
// Node's console drops, aside.
func prettyBrowser(o prettyOptions) Value {
	return &logger{func(fb *Fiber, e *logEntry) Value {
		c := currentConsole(fb)
		first := "[" + o.formatDate(e.date) + "] " + strings.ToUpper(e.level) + " (" + fiberLabel(fb) + ")" +
			joinSpans(spanTexts(fb, e.date)) + ":"
		rest := e.message
		if len(rest) > 0 {
			if s, ok := rest[0].(string); ok {
				first += " " + s
				rest = rest[1:]
			}
		}
		c.group(Some(first))
		defer c.groupEnd()
		if len(e.cause.reasons()) > 0 {
			c.error(pretty(e.cause))
		}
		for _, m := range rest {
			c.log(formatArg(m))
		}
		a := logAnnotations(fb)
		for i, k := range a.keys {
			c.log(k + ": " + formatArg(a.vals[i]))
		}
		return Unit
	}}
}

// LoggerConsolePretty picks the terminal or the browser form by the mode;
// without a choice of colors, colors when stdout is a terminal.
func LoggerConsolePretty(options Value) Value {
	o := record(options)
	colors, chosen := some(o.Get("colors"))
	if !chosen {
		colors = isTTY(os.Stdout.Fd())
	}
	p := prettyOf(colors.(bool), o.Get("formatDate"))
	if ConstructorName(o.Get("mode").(*Data).Tag) == "Browser" {
		if !chosen {
			p.colors = true
		}
		return prettyBrowser(p)
	}
	return prettyTty(p)
}

func LoggerConsolePrettyTty(options Value) Value {
	o := record(options)
	return prettyTty(prettyOf(o.Get("colors").(bool), o.Get("formatDate")))
}

func LoggerConsolePrettyBrowser(options Value) Value {
	o := record(options)
	return prettyBrowser(prettyOf(o.Get("colors").(bool), o.Get("formatDate")))
}
