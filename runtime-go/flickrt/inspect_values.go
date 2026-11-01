package flickrt

// Effect's `Formatter`, `Inspectable` and `Version` over the JSON-like data
// Flick hands them (`Json.to_unknown`).
// shortcut: JSON-like data only, written as JSON.stringify writes it; Effect's own forms of other values (classes, symbols, cycles) when a program formats one

import (
	"bytes"
	"encoding/json"
	"math/big"
	"strconv"
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

// formatSpaced is Effect's `Formatter.format` of a value as the JS target
// holds it, by `space` spaces: an array or an object of more than one
// entry over several lines. A variant is its object of `_tag` and fields;
// an Option, a Result and a Duration write themselves, without spaces.
//
// shortcut: other values of the runtime (Chunk, BigDecimal, DateTime, …) as Format writes them; port their `toString` when a report shows one
func formatSpaced(v Value, space int) string {
	gap := strings.Repeat(" ", space)
	var recur func(v Value, d int) string
	entries := func(open, close string, keys []string, vals []Value, d int) string {
		var b strings.Builder
		b.WriteString(open)
		multiline := gap != "" && len(vals) > 1
		for i, x := range vals {
			if i > 0 {
				b.WriteByte(',')
			}
			if multiline {
				b.WriteString("\n" + strings.Repeat(gap, d+1))
			}
			if keys != nil {
				b.WriteString(QuoteJSON(keys[i]) + ":")
				if multiline {
					b.WriteByte(' ')
				}
			}
			if multiline {
				b.WriteString(recur(x, d+1))
			} else {
				b.WriteString(recur(x, d))
			}
		}
		if multiline {
			b.WriteString("\n" + strings.Repeat(gap, d))
		}
		return b.String() + close
	}
	recur = func(v Value, d int) string {
		switch x := v.(type) {
		case string:
			return QuoteJSON(x)
		case float64, int32, bool:
			return jsString(x)
		case *big.Int:
			return x.String() + "n"
		case []Value:
			return entries("[", "]", nil, x, d)
		case *Record:
			return entries("{", "}", x.keys, x.vals, d)
		case *Redacted:
			return "<redacted>"
		case *Duration:
			return x.String()
		case *Data:
			wrapped := map[string]string{"Some": "some", "Success": "success", "Failure": "failure"}
			switch {
			case x.Tag == "None":
				return "none()"
			case wrapped[x.Tag] != "":
				return wrapped[x.Tag] + "(" + formatSpaced(x.Fields[0], 0) + ")"
			}
			keys, vals := []string{"_tag"}, []Value{x.Tag}
			for i, f := range x.Fields {
				keys, vals = append(keys, "_"+strconv.Itoa(i)), append(vals, f)
			}
			return entries("{", "}", keys, vals, d)
		}
		if v == Unit {
			return "undefined"
		}
		return Format(v)
	}
	return recur(v, 0)
}
