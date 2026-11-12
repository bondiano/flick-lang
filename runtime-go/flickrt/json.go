package flickrt

import (
	"fmt"
	"math"
	"math/big"
	"slices"
	"strconv"
	"strings"
)

// JSON values are what JS's JSON.parse gives, in Go terms: `JsonNull`,
// `bool`, `float64`, `string` (WTF-8), `[]Value` and `*Record` with the
// key order of a JS object.

type jsonNull struct{}

// JsonNull is JSON's `null`.
var JsonNull Value = &jsonNull{}

// JsonParse is `Json.parse`: the value of a JSON text, or a SchemaError
// with the parser's reason.
func JsonParse(text Value) Value {
	v, reason := parseJSON(text.(string))
	if reason != "" {
		return Err(schemaErrorOf([]issue{{"", "not JSON: " + reason}}))
	}
	return Ok(v)
}

// parseJSON is the value of a JSON text, or the reason V8's JSON.parse
// gives for refusing it.
func parseJSON(text string) (Value, string) {
	p := &jsonParser{s: text}
	p.space()
	v, ok := p.value()
	if ok {
		p.space()
		if p.i < len(p.s) {
			p.problem, ok = "Unexpected non-whitespace character after JSON "+p.at(), false
		}
	}
	if !ok {
		return nil, p.reason()
	}
	return v, ""
}

type jsonParser struct {
	s       string
	i       int
	problem string   // what V8 says was expected where parsing stopped
	keys    []string // the keys and values of the arrays and objects being read
	vals    []Value
}

// at is V8's position of the parser, in UTF-16 units, with its line and
// column.
func (p *jsonParser) at() string {
	before := p.s[:min(p.i, len(p.s))]
	line := strings.Count(before, "\n") + 1
	column := Length(before[strings.LastIndexByte(before, '\n')+1:]).(int32) + 1
	return fmt.Sprintf("at position %d (line %d column %d)", Length(before).(int32), line, column)
}

func (p *jsonParser) expect(what string) (Value, bool) {
	p.problem = what + " in JSON " + p.at()
	return nil, false
}

func (p *jsonParser) reason() string {
	if p.problem != "" {
		return p.problem
	}
	if p.i >= len(p.s) {
		return "Unexpected end of JSON input"
	}
	switch p.s {
	case "NaN", "Infinity", "undefined", "[object Object]":
		return `"` + p.s + `" is not valid JSON`
	}
	// V8 names the UTF-16 unit it stopped at and shows the source as it
	// is, cut to 10 units around that place when it is 21 units or more
	const context = 10
	units := utf16Units(p.s)
	pos := int(Length(p.s[:p.i]).(int32))
	around := func(from, to int) string { return fromUnits(units[from:to]) }
	var source string
	switch {
	case len(units) < 2*context+1:
		source = `"` + p.s + `"`
	case pos < context:
		source = `"` + around(0, pos+context) + `"...`
	case pos < len(units)-context:
		source = `..."` + around(pos-context, pos+context) + `"...`
	default:
		source = `..."` + around(pos-context, len(units)) + `"`
	}
	return fmt.Sprintf("Unexpected token '%s', %s is not valid JSON", around(pos, pos+1), source)
}

func (p *jsonParser) space() {
	for p.i < len(p.s) && strings.IndexByte(" \t\n\r", p.s[p.i]) >= 0 {
		p.i++
	}
}

// next is the byte at the parser's position, 0 at the end of the input.
func (p *jsonParser) next() byte {
	if p.i < len(p.s) {
		return p.s[p.i]
	}
	return 0
}

// literal reads `word` as V8 does: unit by unit, refusing the first that
// differs as a token of its own; a word cut short is the end of the input.
func (p *jsonParser) literal(word string, v Value) (Value, bool) {
	for i := 1; i < len(word); i++ {
		p.i++
		switch c := p.next(); {
		case p.i >= len(p.s):
			return nil, false
		case c == word[i]:
		case c == '"':
			return p.expect("Unexpected string")
		case c == '-' || c >= '0' && c <= '9':
			return p.expect("Unexpected number")
		default:
			return nil, false
		}
	}
	p.i++
	return v, true
}

func (p *jsonParser) value() (Value, bool) {
	if p.i >= len(p.s) {
		return nil, false
	}
	switch c := p.s[p.i]; {
	case c == '{':
		return p.object()
	case c == '[':
		return p.array()
	case c == '"':
		v, ok := p.string()
		return v, ok
	case c == 't':
		return p.literal("true", true)
	case c == 'f':
		return p.literal("false", false)
	case c == 'n':
		return p.literal("null", JsonNull)
	case c == '-' || c >= '0' && c <= '9':
		return p.number()
	}
	return nil, false
}

func (p *jsonParser) number() (Value, bool) {
	start := p.i
	digits := func() int {
		n := 0
		for p.i < len(p.s) && p.s[p.i] >= '0' && p.s[p.i] <= '9' {
			p.i++
			n++
		}
		return n
	}
	if p.s[p.i] == '-' {
		p.i++
	}
	if p.i < len(p.s) && p.s[p.i] == '0' {
		p.i++
		// V8 refuses a digit after a leading zero as a number token of its own
		if p.i < len(p.s) && p.s[p.i] >= '0' && p.s[p.i] <= '9' {
			return p.expect("Unexpected number")
		}
	} else if digits() == 0 {
		return p.expect("No number after minus sign")
	}
	if p.i < len(p.s) && p.s[p.i] == '.' {
		p.i++
		if digits() == 0 {
			return p.expect("Unterminated fractional number")
		}
	}
	if p.i < len(p.s) && (p.s[p.i] == 'e' || p.s[p.i] == 'E') {
		p.i++
		if p.i < len(p.s) && (p.s[p.i] == '+' || p.s[p.i] == '-') {
			p.i++
		}
		if digits() == 0 {
			return p.expect("Exponent part is missing a number")
		}
	}
	// beyond float64's range is an infinity, as JS reads it
	x, _ := strconv.ParseFloat(p.s[start:p.i], 64)
	return x, true
}

func (p *jsonParser) string() (string, bool) {
	v, ok := p.quoted()
	if !ok && p.problem == "" {
		p.expect("Unterminated string")
	}
	return v, ok
}

func (p *jsonParser) quoted() (string, bool) {
	p.i++
	var units []uint16
	plain := p.i
	for p.i < len(p.s) {
		c := p.s[p.i]
		switch {
		case c == '"':
			if units == nil {
				s := p.s[plain:p.i]
				p.i++
				return s, true
			}
			p.i++
			return fromUnits(units), true
		case c < 0x20:
			p.expect("Bad control character in string literal")
			return "", false
		case c == '\\':
			if units == nil {
				units = utf16Units(p.s[plain:p.i])
			}
			if p.i+1 >= len(p.s) {
				p.problem = "Unexpected end of JSON input"
				return "", false
			}
			e := p.s[p.i+1]
			p.i += 2
			bad := func() (string, bool) {
				p.i--
				p.expect("Bad escaped character")
				return "", false
			}
			switch e {
			case '"', '\\', '/':
				units = append(units, uint16(e))
			case 'b':
				units = append(units, '\b')
			case 'f':
				units = append(units, '\f')
			case 'n':
				units = append(units, '\n')
			case 'r':
				units = append(units, '\r')
			case 't':
				units = append(units, '\t')
			case 'u':
				// V8 points at the first unit that is not a hex digit
				var n uint16
				for range 4 {
					d := hexDigit(p.s, p.i)
					if d < 0 {
						p.expect("Bad Unicode escape")
						return "", false
					}
					n = n<<4 | uint16(d)
					p.i++
				}
				units = append(units, n)
			default:
				return bad()
			}
		default:
			r, n := decode(p.s[p.i:])
			if units != nil {
				units = append(units, utf16Units(string(encodeRune(nil, r)))...)
			}
			p.i += n
		}
	}
	return "", false
}

func (p *jsonParser) array() (Value, bool) {
	p.i++
	p.space()
	if p.i < len(p.s) && p.s[p.i] == ']' {
		p.i++
		return []Value{}, true
	}
	mark := len(p.vals)
	defer func() { p.vals = p.vals[:mark] }()
	for {
		p.space()
		v, ok := p.value()
		if !ok {
			return nil, false
		}
		p.vals = append(p.vals, v)
		p.space()
		switch p.next() {
		case ',':
			p.i++
		case ']':
			p.i++
			return slices.Clone(p.vals[mark:]), true
		default:
			return p.expect("Expected ',' or ']' after array element")
		}
	}
}

func (p *jsonParser) object() (Value, bool) {
	p.i++
	p.space()
	if p.i < len(p.s) && p.s[p.i] == '}' {
		p.i++
		return &Record{}, true
	}
	keys, mark := len(p.keys), len(p.vals)
	defer func() { p.keys, p.vals = p.keys[:keys], p.vals[:mark] }()
	first := true
	for {
		p.space()
		if p.i >= len(p.s) || p.s[p.i] != '"' {
			if first {
				return p.expect("Expected property name or '}'")
			}
			return p.expect("Expected double-quoted property name")
		}
		first = false
		k, ok := p.string()
		if !ok {
			return nil, false
		}
		p.space()
		if p.i >= len(p.s) || p.s[p.i] != ':' {
			return p.expect("Expected ':' after property name")
		}
		p.i++
		p.space()
		v, ok := p.value()
		if !ok {
			return nil, false
		}
		p.keys, p.vals = append(p.keys, k), append(p.vals, v)
		p.space()
		switch p.next() {
		case ',':
			p.i++
		case '}':
			p.i++
			out := newRecord(len(p.keys) - keys)
			for i, k := range p.keys[keys:] {
				out.put(k, p.vals[mark+i])
			}
			return out, true
		default:
			return p.expect("Expected ',' or '}' after property value")
		}
	}
}

// JsonStringify is JS's JSON.stringify of a JSON value.
func JsonStringify(v Value) Value {
	var b strings.Builder
	stringify(&b, v)
	return b.String()
}

func stringify(b *strings.Builder, v Value) {
	switch x := v.(type) {
	case bool:
		b.WriteString(strconv.FormatBool(x))
	case float64:
		switch {
		case math.IsNaN(x) || math.IsInf(x, 0):
			b.WriteString("null")
		case x == 0:
			b.WriteString("0")
		default:
			b.WriteString(ShowNumber(x))
		}
	case int32:
		b.WriteString(strconv.Itoa(int(x)))
	case string:
		quoteTo(b, x)
	case []Value:
		b.WriteByte('[')
		for i, e := range x {
			if i > 0 {
				b.WriteByte(',')
			}
			stringify(b, e)
		}
		b.WriteByte(']')
	case *Record:
		b.WriteByte('{')
		for i, k := range x.keys {
			if i > 0 {
				b.WriteByte(',')
			}
			quoteTo(b, k)
			b.WriteByte(':')
			stringify(b, x.vals[i])
		}
		b.WriteByte('}')
	default:
		b.WriteString("null")
	}
}

// describeInput is what an issue says it got, as the JS runtime describes an
// input.
func describeInput(in Value) string {
	switch x := in.(type) {
	case *jsonNull:
		return "null"
	case float64:
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return ShowNumber(x)
		}
		return "number"
	case int32:
		return "number"
	case string:
		return "string"
	case bool:
		return "boolean"
	case *big.Int:
		return "bigint"
	case []Value:
		return "an array"
	case *Record, Constructed:
		return "an object"
	case Fn, Fn2, Fn3, Poly:
		return "function"
	}
	if in == Unit {
		return "undefined"
	}
	return "an object"
}

// An issue of decoding or encoding: where, relative to the value being
// decoded, and what.
type issue struct{ path, message string }

// under is the issues of a part of a value at `key`: an index or a label.
func under(key any, issues []issue) []issue {
	out := make([]issue, len(issues))
	for i, is := range issues {
		var at string
		switch k := key.(type) {
		case int:
			at = "[" + strconv.Itoa(k) + "]"
		default:
			at = k.(string)
		}
		switch {
		case is.path == "":
			out[i] = issue{at, is.message}
		case strings.HasPrefix(is.path, "["):
			out[i] = issue{at + is.path, is.message}
		default:
			out[i] = issue{at + "." + is.path, is.message}
		}
	}
	return out
}

var schemaErrorTag = Tag("Prelude.SchemaError.SchemaError")

// schemaErrorOf is the program's SchemaError of issues.
func schemaErrorOf(issues []issue) Value {
	out := make([]Value, len(issues))
	for i, is := range issues {
		path := is.path
		if path == "" {
			path = "(root)"
		}
		out[i] = Rec("message", is.message, "path", path)
	}
	return NewData(schemaErrorTag, out)
}

// hexDigit is the value of the hex digit at s[i], or -1.
func hexDigit(s string, i int) int {
	if i >= len(s) {
		return -1
	}
	switch c := s[i]; {
	case c >= '0' && c <= '9':
		return int(c - '0')
	case c >= 'a' && c <= 'f':
		return int(c-'a') + 10
	case c >= 'A' && c <= 'F':
		return int(c-'A') + 10
	}
	return -1
}
