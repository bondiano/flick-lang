package flickrt

// Effect's `Formatter`, `Inspectable` and `Version` over the JSON-like data
// Flick hands them (`Json.to_unknown`).
// shortcut: JSON-like data only, written as JSON.stringify writes it; Effect's own forms of other values (classes, symbols, cycles) when a program formats one

import (
	"bytes"
	"encoding/json"
	"strings"
)

// jsonIndented is JSON.stringify's text with `indent` spaces, one line for 0.
func jsonIndented(data Value, indent int32) string {
	text := JsonStringify(data).(string)
	if indent <= 0 {
		return text
	}
	var b bytes.Buffer
	if json.Indent(&b, []byte(text), "", strings.Repeat(" ", int(indent))) != nil {
		return text
	}
	return b.String()
}

func FormatterFormat(data Value) Value { return JsonStringify(data) }

func FormatterFormatJson(indent, data Value) Value { return jsonIndented(data, indent.(int32)) }

// InspectableToStringUnknown is a string as it is, anything else as JSON.
func InspectableToStringUnknown(indent, data Value) Value {
	if s, ok := data.(string); ok {
		return s
	}
	return jsonIndented(data, indent.(int32))
}

func InspectableToJson(data Value) Value { return data }

// VersionCurrent is the version of Effect the runtime follows.
func VersionCurrent() Value { return "4.0.5" }
