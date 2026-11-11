package flickrt

import (
	"slices"
	"strconv"
	"strings"
)

// The pretty form of a cause and its pretty errors: each failure and
// defect as an Error, and an interruption alone as an `InterruptError`
// caused by the interrupting fibers.

// prettyError is the Error made of a reason: its name, message and stack,
// and the error it was caused by.
type prettyError struct {
	name, message, stack string
	cause                *prettyError
}

const interruptedMessage = "All fibers interrupted without error"

func prettyErrors(c *Cause) []*prettyError {
	var out []*prettyError
	var interrupts []*Cause
	for _, r := range c.reasons() {
		switch r.kind {
		case causeInterrupt:
			interrupts = append(interrupts, r)
		case causeFail:
			out = append(out, prettyFailure(r.err, r.trace))
		case causeDie:
			if _, ok := r.err.(*Data); ok {
				out = append(out, prettyFailure(r.err, r.trace))
			} else {
				out = append(out, prettyDefect(r.defect, r.trace))
			}
		}
	}
	if len(out) > 0 || len(interrupts) == 0 {
		return out
	}
	lines := []string{"InterruptCause: The fiber was interrupted by:"}
	for _, r := range interrupts {
		id := "unknown"
		if r.fiber != 0 {
			id = "#" + strconv.FormatInt(r.fiber, 10)
		}
		lines = append(lines, "    at fiber ("+id+")")
	}
	cause := &prettyError{"InterruptCause", "The fiber was interrupted by:", strings.Join(lines, "\n"), nil}
	stack := withTrace("InterruptError: "+interruptedMessage, interrupts[0].trace)
	return []*prettyError{{"InterruptError", interruptedMessage, stack, cause}}
}

// prettyFailure is the Error of a typed error: a record's `message` and
// `name`, as an object's, else its JSON.
func prettyFailure(err Value, trace *stackFrame) *prettyError {
	name, message := "Error", formatJSON(err)
	if r, ok := err.(*Record); ok {
		if m, ok := optField(r, "message", nil).(string); ok {
			message = m
		}
		if n, ok := optField(r, "name", nil).(string); ok {
			name = n
		}
	}
	return &prettyError{name, message, withTrace(name+": "+message, trace), nil}
}

// prettyDefect is the Error of a defect: a thrown error keeps its stack,
// here the Flick lines it unwound; a message is an Error of it.
//
// shortcut: a thrown error's stack has the Flick lines, where the JS target's has JS frames; both name the error's message first
func prettyDefect(defect string, trace *stackFrame) *prettyError {
	text := strings.TrimSuffix(defect, "\n")
	first, _, _ := strings.Cut(text, "\n")
	if name, message, ok := strings.Cut(first, ": "); ok && slices.Contains(jsErrorNames, name) {
		return &prettyError{name, message, withTrace(text, trace), nil}
	}
	if text == "" {
		text = "Unknown error: "
	}
	return &prettyError{"Error", text, withTrace("Error: "+text, trace), nil}
}

// withTrace is a stack with the frames of the stack trace annotated on
// its reason, at most ten of them.
func withTrace(stack string, frame *stackFrame) string {
	for i := 0; frame != nil && i < 10; i++ {
		if frame.stack != "" {
			stack += "\n    at " + frame.name + " (" + strings.TrimPrefix(frame.stack, "at ") + ")"
		} else {
			stack += "\n    at " + frame.name
		}
		p, _ := some(frame.parent)
		frame, _ = p.(*stackFrame)
	}
	return stack
}

// causePretty is the rendering of each pretty error, its cause nested.
func causePretty(c *Cause) string {
	errs := prettyErrors(c)
	out := make([]string, len(errs))
	for i, e := range errs {
		out[i] = e.rendered()
	}
	return strings.Join(out, "\n")
}

// rendered is the stack, with the stack of its cause nested under
// `[cause]`.
func (e *prettyError) rendered() string {
	if e.cause == nil {
		return e.stack
	}
	lines := strings.Split(e.cause.stack, "\n")
	return e.stack + " {\n  [cause]: " + strings.Join(lines, "\n  ") + "\n}"
}

// formatJSON is the JSON of a Flick value, with an Option or a Result as
// their `toJSON` gives them.
func formatJSON(v Value) string {
	switch x := v.(type) {
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			parts[i] = formatJSON(e)
		}
		return "[" + strings.Join(parts, ",") + "]"
	case *Data:
		switch x.Tag {
		case "None":
			return `{"_id":"Option","_tag":"None"}`
		case "Some":
			return `{"_id":"Option","_tag":"Some","value":` + formatJSON(x.Fields()[0]) + "}"
		case "Success":
			return `{"_id":"Result","_tag":"Success","value":` + formatJSON(x.Fields()[0]) + "}"
		case "Failure":
			return `{"_id":"Result","_tag":"Failure","failure":` + formatJSON(x.Fields()[0]) + "}"
		}
		out := `{"_tag":` + QuoteJSON(x.Tag)
		for i, f := range x.Fields() {
			out += "," + QuoteJSON(nativeField(x.Tag, i)) + ":" + formatJSON(f)
		}
		return out + "}"
	case *Record:
		parts := make([]string, len(x.keys))
		for i, k := range x.keys {
			parts[i] = QuoteJSON(k) + ":" + formatJSON(x.vals[i])
		}
		return "{" + strings.Join(parts, ",") + "}"
	}
	return Show(v).(string)
}
