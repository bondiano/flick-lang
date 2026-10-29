package flickrt

import (
	"fmt"
	"math"
	"math/big"
	"strconv"
	"strings"
	"unicode/utf8"
)

// JSON values are what JS's JSON.parse gives, in Go terms: `JsonNull`,
// `bool`, `float64`, `string` (WTF-8), `[]Value` and `*Record` with the
// key order of a JS object.

type jsonNull struct{}

// JsonNull is JSON's `null`.
var JsonNull Value = &jsonNull{}

// JsonParse is `Json.parse`: the value of a JSON text, or a SchemaError
// with the parser's reason.
//
// shortcut: syntax errors read as V8's for objects, strings, a bad token and the end of input; port V8's other texts if a program shows them
func JsonParse(text Value) Value {
	p := &jsonParser{s: text.(string)}
	p.space()
	v, ok := p.value()
	if ok {
		p.space()
		if p.i < len(p.s) {
			p.problem, ok = "Unexpected non-whitespace character after JSON "+p.at(), false
		}
	}
	if !ok {
		return Err(schemaErrorOf([]issue{{"", "not JSON: " + p.reason()}}))
	}
	return Ok(v)
}

type jsonParser struct {
	s       string
	i       int
	problem string // what V8 says was expected where parsing stopped
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
	r, _ := utf8.DecodeRuneInString(p.s[p.i:])
	return fmt.Sprintf("Unexpected token '%c', %s is not valid JSON", r, QuoteJSON(p.s))
}

func (p *jsonParser) space() {
	for p.i < len(p.s) && strings.IndexByte(" \t\n\r", p.s[p.i]) >= 0 {
		p.i++
	}
}

func (p *jsonParser) literal(word string, v Value) (Value, bool) {
	if strings.HasPrefix(p.s[p.i:], word) {
		p.i += len(word)
		return v, true
	}
	if strings.HasPrefix(word, p.s[p.i:]) {
		p.i = len(p.s) // a word cut short is the end of the input
	}
	return nil, false
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
		return p.string()
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
	} else if digits() == 0 {
		return nil, false
	}
	if p.i < len(p.s) && p.s[p.i] == '.' {
		p.i++
		if digits() == 0 {
			return nil, false
		}
	}
	if p.i < len(p.s) && (p.s[p.i] == 'e' || p.s[p.i] == 'E') {
		p.i++
		if p.i < len(p.s) && (p.s[p.i] == '+' || p.s[p.i] == '-') {
			p.i++
		}
		if digits() == 0 {
			return nil, false
		}
	}
	// beyond float64's range is an infinity, as JS reads it
	x, _ := strconv.ParseFloat(p.s[start:p.i], 64)
	return x, true
}

func (p *jsonParser) string() (Value, bool) {
	v, ok := p.quoted()
	if !ok && p.problem == "" {
		return p.expect("Unterminated string")
	}
	return v, ok
}

func (p *jsonParser) quoted() (Value, bool) {
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
			return p.expect("Bad control character in string literal")
		case c == '\\':
			if units == nil {
				units = utf16Units(p.s[plain:p.i])
			}
			if p.i+1 >= len(p.s) {
				p.i = len(p.s)
				return nil, false
			}
			e := p.s[p.i+1]
			p.i += 2
			bad := func() (Value, bool) {
				p.i--
				return p.expect("Bad escaped character")
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
				if p.i+4 > len(p.s) {
					p.i = len(p.s)
					return nil, false
				}
				n, err := strconv.ParseUint(p.s[p.i:p.i+4], 16, 16)
				if err != nil {
					return p.expect("Bad Unicode escape")
				}
				units = append(units, uint16(n))
				p.i += 4
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
	return nil, false
}

func (p *jsonParser) array() (Value, bool) {
	p.i++
	out := []Value{}
	p.space()
	if p.i < len(p.s) && p.s[p.i] == ']' {
		p.i++
		return out, true
	}
	for {
		p.space()
		v, ok := p.value()
		if !ok {
			return nil, false
		}
		out = append(out, v)
		p.space()
		if p.i >= len(p.s) {
			return nil, false
		}
		switch p.s[p.i] {
		case ',':
			p.i++
		case ']':
			p.i++
			return out, true
		default:
			return p.expect("Expected ',' or ']' after array element")
		}
	}
}

func (p *jsonParser) object() (Value, bool) {
	p.i++
	out := &Record{}
	p.space()
	if p.i < len(p.s) && p.s[p.i] == '}' {
		p.i++
		return out, true
	}
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
		out = out.set(k.(string), v)
		p.space()
		if p.i >= len(p.s) {
			return nil, false
		}
		switch p.s[p.i] {
		case ',':
			p.i++
		case '}':
			p.i++
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
		b.WriteString(QuoteJSON(x))
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
			b.WriteString(QuoteJSON(k))
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
	case *Record, *Data:
		return "an object"
	case Fn, Poly:
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
