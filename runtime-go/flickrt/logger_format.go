package flickrt

import (
	"os"
	"strconv"
	"strings"
	"time"
	"unicode"
)

// The built-in loggers: logfmt, structured records and JSON, the pretty
// consoles, the default logger and the tracer logger.

var (
	LoggerDefault          Value = &logger{logDefault}
	LoggerTracer           Value = &logger{logTracer}
	LoggerFormatSimple     Value = &logger{func(fb *Fiber, m *logEntry) Value { return logfmt(fb, m, escapeDoubleQuotes) }}
	LoggerFormatLogFmt     Value = &logger{func(fb *Fiber, m *logEntry) Value { return logfmt(fb, m, QuoteJSON) }}
	LoggerFormatStructured Value = &logger{func(fb *Fiber, m *logEntry) Value { return structuredOf(fb, m).record() }}
	LoggerFormatJson       Value = &logger{func(fb *Fiber, m *logEntry) Value { return structuredOf(fb, m).json() }}
	LoggerConsoleLogFmt          = LoggerWithConsoleLog(LoggerFormatLogFmt)
	LoggerConsoleJson            = LoggerWithConsoleLog(LoggerFormatJson)
	// consoleStructured writes the structured object as Node inspects it
	LoggerConsoleStructured Value = &logger{func(fb *Fiber, m *logEntry) Value {
		currentConsole(fb).log(structuredOf(fb, m).inspect())
		return Unit
	}}
)

// formatLabel is a label with whitespace, `=` and `"` as `_`.
func formatLabel(key string) string {
	return strings.Map(func(r rune) rune {
		if r == '=' || r == '"' || unicode.IsSpace(r) || r == 0xfeff {
			return '_'
		}
		return r
	}, key)
}

func formatLogSpan(label string, start, now float64) string {
	return formatLabel(label) + "=" + ShowNumber(now-start) + "ms"
}

// escapeDoubleQuotes is formatSimple's quoting: a `"` not after a
// backslash gets one.
func escapeDoubleQuotes(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for i := 0; i < len(s); i++ {
		switch {
		case s[i] == '\\' && i+1 < len(s):
			_, n := decode(s[i+1:])
			b.WriteString(s[i : i+1+n])
			i += n
		case s[i] == '"':
			b.WriteString(`\"`)
		default:
			b.WriteByte(s[i])
		}
	}
	b.WriteByte('"')
	return b.String()
}

func isoDate(millis float64) string {
	return time.UnixMilli(int64(millis)).UTC().Format("2006-01-02T15:04:05.000Z")
}

// defaultDateFormat is the local time of day, to the millisecond.
func defaultDateFormat(millis float64) string {
	return time.UnixMilli(int64(millis)).Local().Format("15:04:05.000")
}

type logSpan struct {
	label string
	start float64
}

func logSpansOf(fb *Fiber) []logSpan {
	if s, ok := fb.env.get(logSpansKey); ok {
		return s.([]logSpan)
	}
	return nil
}

func logAnnotationsOf(fb *Fiber) *Record {
	return envOr(fb, logAnnotationsKey, noAttributes).(*Record)
}

func fiberLabel(fb *Fiber) string { return "#" + strconv.FormatInt(fb.state.id, 10) }

// logfmt is the format of the simple and logfmt loggers, which quote a
// value with whitespace, `"` or `=` by `quote`.
func logfmt(fb *Fiber, m *logEntry, quote func(string) string) string {
	value := func(v string) string {
		if strings.ContainsAny(v, "\"=") || strings.IndexFunc(v, unicode.IsSpace) >= 0 {
			return quote(v)
		}
		return v
	}
	var b strings.Builder
	field := func(label, v string) {
		if b.Len() > 0 {
			b.WriteByte(' ')
		}
		b.WriteString(formatLabel(label) + "=" + value(v))
	}
	field("timestamp", isoDate(m.date))
	field("level", strings.ToUpper(m.level))
	field("fiber", fiberLabel(fb))
	for _, part := range m.message {
		field("message", attrText(part))
	}
	if m.cause != nil {
		field("cause", causePretty(m.cause))
	}
	for _, s := range logSpansOf(fb) {
		b.WriteString(" " + formatLogSpan(s.label, s.start, m.date))
	}
	a := logAnnotationsOf(fb)
	for i, k := range a.keys {
		field(k, attrText(a.vals[i]))
	}
	return b.String()
}

// structuredLog is the object of the structured format.
type structuredLog struct {
	message     []Value
	level       string
	timestamp   string
	cause       Value // an Option String
	annotations *Record
	spans       *Record
	fiberId     string
}

func structuredOf(fb *Fiber, m *logEntry) *structuredLog {
	spans := newRecord(0)
	for _, s := range logSpansOf(fb) {
		if spans.index(s.label) < 0 {
			spans.put(s.label, m.date-s.start)
		}
	}
	cause := None
	if m.cause != nil {
		cause = Some(causePretty(m.cause))
	}
	return &structuredLog{messageParts(m.message), strings.ToUpper(m.level), isoDate(m.date), cause,
		logAnnotationsOf(fb), spans, fiberLabel(fb)}
}

// record is `Logger.Structured`.
func (s *structuredLog) record() Value {
	pairs := func(r *Record) []Value {
		out := make([]Value, len(r.keys))
		for i, k := range r.keys {
			out[i] = []Value{k, r.vals[i]}
		}
		return out
	}
	return Rec("level", s.level, "fiberId", s.fiberId, "timestamp", s.timestamp, "message", s.message,
		"cause", s.cause, "annotations", pairs(s.annotations), "spans", pairs(s.spans))
}

// json is the JSON text of the object.
func (s *structuredLog) json() string {
	object := func(r *Record) string {
		parts := make([]string, len(r.keys))
		for i, k := range r.keys {
			v := QuoteJSON(attrText(r.vals[i]))
			if n, ok := r.vals[i].(float64); ok {
				v = jsonNumber(n)
			}
			parts[i] = QuoteJSON(k) + ":" + v
		}
		return "{" + strings.Join(parts, ",") + "}"
	}
	message := QuoteJSON(s.message[0].(string))
	if len(s.message) != 1 {
		parts := make([]string, len(s.message))
		for i, part := range s.message {
			parts[i] = QuoteJSON(part.(string))
		}
		message = "[" + strings.Join(parts, ",") + "]"
	}
	out := `{"message":` + message + `,"level":` + QuoteJSON(s.level) + `,"timestamp":` + QuoteJSON(s.timestamp)
	if c, ok := some(s.cause); ok {
		out += `,"cause":` + QuoteJSON(c.(string))
	}
	return out + `,"annotations":` + object(s.annotations) + `,"spans":` + object(s.spans) +
		`,"fiberId":` + QuoteJSON(s.fiberId) + "}"
}

// jsonNumber is JSON.stringify of a number: null when it is not finite.
func jsonNumber(n float64) string {
	if n-n != 0 {
		return "null"
	}
	return ShowNumber(n)
}

// inspect is Node's inspection of the object, as `console.log` writes it.
func (s *structuredLog) inspect() string {
	object := func(r *Record) string {
		entries := make([]string, len(r.keys))
		for i, k := range r.keys {
			entries[i] = inspectKey(k) + ": " + inspect(r.vals[i])
		}
		return inspectEntries(entries, 2)
	}
	message := inspect(s.message[0])
	if len(s.message) != 1 {
		message = inspect(s.message)
	}
	cause := "undefined"
	if c, ok := some(s.cause); ok {
		cause = inspect(c)
	}
	return inspectEntries([]string{"message: " + message, "level: " + inspect(s.level),
		"timestamp: " + inspect(s.timestamp), "cause: " + cause, "annotations: " + object(s.annotations),
		"spans: " + object(s.spans), "fiberId: " + inspect(s.fiberId)}, 0)
}

// inspectKey is a key as Node writes it: quoted unless an identifier.
func inspectKey(k string) string {
	for i, r := range k {
		if !(r == '_' || r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || i > 0 && r >= '0' && r <= '9') {
			return inspect(k)
		}
	}
	if k == "" {
		return "''"
	}
	return k
}

// inspectEntries is Node's reduceToSingleString of an object's entries at
// an indentation: on one line when it fits 80 columns, else one per line.
func inspectEntries(entries []string, indentation int) string {
	if len(entries) == 0 {
		return "{}"
	}
	start := len(entries) + indentation + 1 + 10
	total := len(entries) + start
	fits := total+len(entries) <= 80
	for _, e := range entries {
		total += len(e)
		fits = fits && total <= 80
	}
	joined := strings.Join(entries, ", ")
	if fits && !strings.Contains(joined, "\n") {
		return "{ " + joined + " }"
	}
	indent := "\n" + strings.Repeat(" ", indentation)
	return "{" + indent + "  " + strings.Join(entries, ","+indent+"  ") + indent + "}"
}

// logDefault is the default logger: `[time] LEVEL (#fiber) spans:` and the
// message, with the annotations, on the console.
func logDefault(fb *Fiber, m *logEntry) Value {
	args := append([]Value{}, m.message...)
	if m.cause != nil {
		args = append(args, causePretty(m.cause))
	}
	spans := ""
	for _, s := range logSpansOf(fb) {
		spans += " " + formatLogSpan(s.label, s.start, m.date)
	}
	if a := logAnnotationsOf(fb); len(a.keys) > 0 {
		args = append(args, a)
	}
	head := "[" + defaultDateFormat(m.date) + "] " + strings.ToUpper(m.level) + " (" + fiberLabel(fb) + ")" + spans + ":"
	write := currentConsole(fb).log
	if envOr(fb, logToStderrKey, false).(bool) {
		write = currentConsole(fb).warn
	}
	write(consoleText(append([]Value{head}, args...)...))
	return Unit
}

// logTracer is the tracer logger: the message as an event of the fiber's
// span.
func logTracer(fb *Fiber, m *logEntry) Value {
	s := currentSpanLocal(fb)
	if s == nil {
		return Unit
	}
	a := logAnnotationsOf(fb)
	attrs := make([]spanAttr, 0, len(a.keys)+3)
	for i, k := range a.keys {
		attrs = append(attrs, spanAttr{k, a.vals[i]})
	}
	attrs = append(attrs, spanAttr{"effect.fiberId", float64(fb.state.id)},
		spanAttr{"effect.logLevel", strings.ToUpper(m.level)})
	if m.cause != nil {
		attrs = append(attrs, spanAttr{"effect.cause", causePretty(m.cause)})
	}
	name := consoleText(m.message...)
	if len(m.message) != 1 {
		name = strings.Join(stringsOf(messageParts(m.message)), ",")
	}
	s.event(name, clockOf(fb).nanos(), attrs)
	return Unit
}

var logLevelColors = map[string][]string{
	"Trace": {"90"}, "Debug": {"34"}, "Info": {"32"}, "Warn": {"33"}, "Error": {"31"}, "Fatal": {"101", "30"},
}

func withColor(text string, colors ...string) string {
	out := ""
	for _, c := range colors {
		out += "\x1b[" + c + "m"
	}
	return out + text + "\x1b[0m"
}

func withoutColor(text string, _ ...string) string { return text }

// prettyDate is the `formatDate` of the options, the default without one.
func prettyDate(fb *Fiber, formatDate Value) func(float64) string {
	if f, ok := some(formatDate); ok {
		return func(millis float64) string { return Apply(fb, f, millis).(string) }
	}
	return defaultDateFormat
}

// prettyTty is the pretty logger of a terminal: a first line, then the
// rest of the message and the annotations in a console group.
func prettyTty(colors bool, formatDate Value) Value {
	color := withoutColor
	if colors {
		color = withColor
	}
	return &logger{func(fb *Fiber, m *logEntry) Value {
		c := currentConsole(fb)
		write := c.log
		if envOr(fb, logToStderrKey, false).(bool) {
			write = c.warn
		}
		first := color("["+prettyDate(fb, formatDate)(m.date)+"]", "37") + " " +
			color(strings.ToUpper(m.level), logLevelColors[m.level]...) + " (" + fiberLabel(fb) + ")"
		for _, s := range logSpansOf(fb) {
			first += " " + formatLogSpan(s.label, s.start, m.date)
		}
		first += ":"
		rest := m.message
		if len(rest) > 0 {
			if text, ok := rest[0].(string); ok {
				first += " " + color(text, "1", "36")
				rest = rest[1:]
			}
		}
		write(first)
		c.group(Rec("label", None))
		defer c.groupEnd()
		if m.cause != nil {
			write(causePretty(m.cause))
		}
		for _, part := range rest {
			write(consoleText(part))
		}
		a := logAnnotationsOf(fb)
		for i, k := range a.keys {
			write(consoleText(color(k+":", "1", "37"), a.vals[i]))
		}
		return Unit
	}}
}

// prettyBrowser is the pretty logger of a browser on Node's console, which
// drops its `%c` styles: a collapsed group of the first line.
func prettyBrowser(formatDate Value) Value {
	return &logger{func(fb *Fiber, m *logEntry) Value {
		c := currentConsole(fb)
		first := "[" + prettyDate(fb, formatDate)(m.date) + "] " + strings.ToUpper(m.level) + " (" + fiberLabel(fb) + ")"
		for _, s := range logSpansOf(fb) {
			first += " " + formatLogSpan(s.label, s.start, m.date)
		}
		first += ":"
		rest := m.message
		if len(rest) > 0 {
			if text, ok := rest[0].(string); ok {
				first += " " + text
				rest = rest[1:]
			}
		}
		c.group(Rec("label", Some(first)))
		defer c.groupEnd()
		if m.cause != nil {
			c.warn(causePretty(m.cause))
		}
		for _, part := range rest {
			c.log(consoleText(part))
		}
		a := logAnnotationsOf(fb)
		for i, k := range a.keys {
			c.log(consoleText(k+":", a.vals[i]))
		}
		return Unit
	}}
}

var prettyModeTag = map[string]string{Tag("Logger.PrettyMode.Browser"): "browser", Tag("Logger.PrettyMode.Tty"): "tty"}

// LoggerConsolePretty is `Logger.console_pretty`: Node has a stdout, so
// `Auto` is the terminal's logger, colored when stdout is a terminal.
func LoggerConsolePretty(options Value) Value {
	o := record(options)
	if prettyModeTag[o.Get("mode").(*Data).Tag] == "browser" {
		return prettyBrowser(o.Get("formatDate"))
	}
	colors := fileIsTerminal(os.Stdout)
	if c, ok := some(o.Get("colors")); ok {
		colors = c.(bool)
	}
	return prettyTty(colors, o.Get("formatDate"))
}

func LoggerConsolePrettyTty(options Value) Value {
	o := record(options)
	return prettyTty(o.Get("colors").(bool), o.Get("formatDate"))
}

func LoggerConsolePrettyBrowser(options Value) Value {
	return prettyBrowser(record(options).Get("formatDate"))
}
