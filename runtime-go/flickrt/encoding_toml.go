package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
	"unicode/utf8"
)

// parseToml is Effect's encoding/Toml `parse` (based on `toml` 4.1.2). An
// offset date-time is the ISO string JSON writes of its `Date`; local
// dates and times stay strings.
func parseToml(input string) (v Value, err string) {
	p := &tomlParser{in: strings.TrimPrefix(input, "\uFEFF"), line: 1, column: 1, explicit: map[*Record]bool{}}
	p.root = &Record{}
	p.current = p.root
	defer func() {
		if r := recover(); r != nil {
			e, ok := r.(encodingError)
			if !ok {
				panic(r)
			}
			v, err = nil, string(e)
		}
	}()
	return p.parse(), ""
}

// encodingError is what a parser of this port throws: the text of a JS
// error, recovered where the parse began.
type encodingError string

// tomlParser reads `in` by code point; `column` counts UTF-16 code units
// as the JS parser's does.
type tomlParser struct {
	in            string
	i             int
	line, column  int
	root, current *Record
	explicit      map[*Record]bool
}

func (p *tomlParser) parse() *Record {
	for {
		p.skipWhitespace()
		if p.done() {
			return p.root
		}
		if p.peek() == '[' {
			p.parseHeader()
		} else {
			keys := p.parseKeyPath('=')
			p.skipInlineWhitespace()
			p.expect('=')
			p.skipInlineWhitespace()
			p.assign(p.current, keys, p.parseValue())
			p.finishStatement()
		}
	}
}

func (p *tomlParser) parseHeader() {
	p.expect('[')
	array := p.peek() == '['
	if array {
		p.advance(1)
	}
	p.skipInlineWhitespace()
	path := p.parseKeyPath(']')
	p.skipInlineWhitespace()
	p.expect(']')
	if array {
		p.expect(']')
	}
	p.finishStatement()

	table := p.resolveTable(path, array)
	if !array && p.explicit[table] {
		p.fail("Cannot redefine table '" + strings.Join(path, ".") + "'")
	}
	if !array {
		p.explicit[table] = true
	}
	p.current = table
}

func (p *tomlParser) resolveTable(path []string, array bool) *Record {
	table := p.root
	for i, key := range path {
		value := lookup(table, key)
		if i == len(path)-1 && array {
			if value == nil {
				value = []Value{}
			}
			list, ok := value.([]Value)
			if !ok {
				p.fail("Cannot redefine existing key '" + strings.Join(path[:i+1], ".") + "'")
			}
			next := &Record{}
			table.put(key, append(list, next))
			return next
		}
		if value == nil {
			value = &Record{}
			table.put(key, value)
		}
		if list, ok := value.([]Value); ok {
			value = nil
			if len(list) > 0 {
				value = list[len(list)-1]
			}
		}
		next, ok := value.(*Record)
		if !ok {
			p.fail("Cannot redefine existing key '" + strings.Join(path[:i+1], ".") + "'")
		}
		table = next
	}
	return table
}

func (p *tomlParser) assign(target *Record, keys []string, value Value) {
	table := target
	for i, key := range keys[:len(keys)-1] {
		switch existing := lookup(table, key).(type) {
		case nil:
			child := &Record{}
			table.put(key, child)
			table = child
		case *Record:
			table = existing
		default:
			p.fail("Cannot redefine existing key '" + strings.Join(keys[:i+1], ".") + "'")
		}
	}
	key := keys[len(keys)-1]
	if has(table, key) {
		p.fail("Cannot redefine existing key '" + strings.Join(keys, ".") + "'")
	}
	table.put(key, value)
}

func isTomlBareKey(c rune) bool {
	return c >= 'A' && c <= 'Z' || c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || c == '_' || c == '-'
}

func (p *tomlParser) parseKeyPath(stop rune) []string {
	var keys []string
	for {
		p.skipInlineWhitespace()
		var key string
		switch p.peek() {
		case '"':
			key = p.parseBasicString(false)
		case '\'':
			key = p.parseLiteralString(false)
		default:
			start := p.i
			for isTomlBareKey(p.peek()) {
				p.advance(1)
			}
			key = p.in[start:p.i]
		}
		if key == "" {
			p.fail("Expected a key")
		}
		keys = append(keys, key)
		p.skipInlineWhitespace()
		if p.peek() == '.' {
			p.advance(1)
			continue
		}
		if p.peek() != stop {
			p.fail("Expected '" + string(stop) + "'")
		}
		return keys
	}
}

var (
	tomlSpacedDateTime = regexp.MustCompile(`^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})?)`)
	tomlOffsetDateTime = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})$`)
	tomlLocalDateTime  = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(?:\.\d+)?$|^\d{4}-\d{2}-\d{2}$|^\d{2}:\d{2}:\d{2}(?:\.\d+)?$`)
	tomlSpecial        = regexp.MustCompile(`^[+-]?(?:inf|nan)$`)
	tomlHex            = regexp.MustCompile(`^0x[0-9A-Fa-f](?:_?[0-9A-Fa-f])*$`)
	tomlOctal          = regexp.MustCompile(`^0o[0-7](?:_?[0-7])*$`)
	tomlBinary         = regexp.MustCompile(`^0b[01](?:_?[01])*$`)
	tomlInteger        = regexp.MustCompile(`^[+-]?(?:0|[1-9](?:_?\d)*)$`)
	tomlFloat          = regexp.MustCompile(`^[+-]?(?:(?:0|[1-9](?:_?\d)*)\.\d(?:_?\d)*(?:[eE][+-]?\d(?:_?\d)*)?|(?:0|[1-9](?:_?\d)*)[eE][+-]?\d(?:_?\d)*)$`)
)

func (p *tomlParser) parseValue() Value {
	switch p.peek() {
	case '"':
		return p.parseBasicString(strings.HasPrefix(p.rest(), `"""`))
	case '\'':
		return p.parseLiteralString(strings.HasPrefix(p.rest(), "'''"))
	case '[':
		return p.parseArray()
	case '{':
		return p.parseInlineTable()
	}
	if m := tomlSpacedDateTime.FindStringSubmatch(p.rest()); m != nil {
		p.advance(len(m[0]))
		v, _ := p.parseDate(m[1] + "T" + m[2])
		return v
	}
	start := p.i
	for !p.done() && !isJSSpace(p.peek()) && !strings.ContainsRune(",#]}", p.peek()) {
		p.advance(1)
	}
	token := p.in[start:p.i]
	switch token {
	case "true":
		return true
	case "false":
		return false
	case "":
		p.fail("Expected a value")
	}
	if v, ok := p.parseDate(token); ok {
		return v
	}
	if n, ok := tomlNumber(token); ok {
		return n
	}
	p.fail("Invalid value '" + token + "'")
	return nil
}

func (p *tomlParser) parseDate(token string) (Value, bool) {
	if tomlOffsetDateTime.MatchString(token) {
		ms := parseJsDate(strings.Replace(strings.Replace(token, "t", "T", 1), "z", "Z", 1))
		if math.IsNaN(ms) {
			p.fail("Invalid date-time '" + token + "'")
		}
		return newJsDate(ms).isoString(), true
	}
	if tomlLocalDateTime.MatchString(token) {
		return strings.Replace(token, "t", "T", 1), true
	}
	return nil, false
}

func tomlNumber(token string) (float64, bool) {
	normalized := strings.ReplaceAll(token, "_", "")
	switch {
	case tomlSpecial.MatchString(token):
		if strings.HasSuffix(normalized, "nan") {
			return math.NaN(), true
		}
		if normalized[0] == '-' {
			return math.Inf(-1), true
		}
		return math.Inf(1), true
	case tomlHex.MatchString(token):
		return parseIntRadix(normalized[2:], 16), true
	case tomlOctal.MatchString(token):
		return parseIntRadix(normalized[2:], 8), true
	case tomlBinary.MatchString(token):
		return parseIntRadix(normalized[2:], 2), true
	case tomlInteger.MatchString(token) || tomlFloat.MatchString(token):
		n, _ := strconv.ParseFloat(normalized, 64) // out of range is ±Infinity, as Number gives
		return n, true
	}
	return 0, false
}

// parseIntRadix is `Number.parseInt` of digits of a power-of-two radix
// (exactly rounded, as V8 rounds them).
func parseIntRadix(digits string, radix int) float64 {
	n, _ := new(big.Int).SetString(digits, radix)
	f, _ := new(big.Float).SetInt(n).Float64()
	return f
}

// appendCodePoint is `out + String.fromCodePoint(r)`: a low surrogate
// after a high one joins it as a pair.
func appendCodePoint(out []byte, r rune) []byte {
	if r >= 0xDC00 && r <= 0xDFFF && len(out) >= 3 {
		if hi, n := decode(string(out[len(out)-3:])); n == 3 && hi >= 0xD800 && hi <= 0xDBFF {
			return utf8.AppendRune(out[:len(out)-3], 0x10000+(hi-0xD800)<<10+r-0xDC00)
		}
	}
	return encodeRune(out, r)
}

// hexCodePoint is the code point of `length` hex digits at the start of
// `s`, or ok false; past U+10FFFF it throws fromCodePoint's RangeError.
func hexCodePoint(s string, length int) (rune, bool) {
	if len(s) < length {
		return 0, false
	}
	n, err := strconv.ParseUint(s[:length], 16, 32)
	if err != nil {
		return 0, false
	}
	if n > 0x10FFFF {
		panic(encodingError("RangeError: Invalid code point " + strconv.FormatUint(n, 10)))
	}
	return rune(n), true
}

var tomlEscapes = map[rune]byte{'b': '\b', 't': '\t', 'n': '\n', 'f': '\f', 'r': '\r', '"': '"', '\\': '\\'}

func (p *tomlParser) parseBasicString(multiline bool) string {
	p.expect('"')
	if multiline {
		p.expect('"')
		p.expect('"')
		p.skipNewline()
	}
	var out []byte
	for !p.done() {
		if multiline && strings.HasPrefix(p.rest(), `"""`) {
			p.advance(3)
			return string(out)
		}
		c := p.peek()
		if !multiline && c == '"' {
			p.advance(1)
			return string(out)
		}
		if !multiline && (c == '\n' || c == '\r') {
			p.fail("Basic strings cannot contain newlines")
		}
		if c != '\\' {
			start := p.i
			p.advance(1)
			out = append(out, p.in[start:p.i]...)
			continue
		}
		p.advance(1)
		if multiline && strings.ContainsRune(" \t\r\n", p.peek()) {
			for p.peek() == ' ' || p.peek() == '\t' {
				p.advance(1)
			}
			if p.peek() != '\n' && p.peek() != '\r' {
				p.fail("Invalid multiline string continuation")
			}
			for strings.ContainsRune(" \t\r\n", p.peek()) {
				p.advance(1)
			}
			continue
		}
		start := p.i
		escape := p.peek()
		p.advance(1)
		if e, ok := tomlEscapes[escape]; ok {
			out = append(out, e)
		} else if escape == 'u' || escape == 'U' {
			length := 8
			if escape == 'u' {
				length = 4
			}
			r, ok := hexCodePoint(p.rest(), length)
			if !ok {
				p.fail("Invalid unicode escape")
			}
			out = appendCodePoint(out, r)
			p.advance(length)
		} else {
			p.fail("Invalid escape '\\" + p.in[start:min(p.i, len(p.in))] + "'")
		}
	}
	p.fail("Unterminated basic string")
	return ""
}

func (p *tomlParser) parseLiteralString(multiline bool) string {
	p.expect('\'')
	if multiline {
		p.expect('\'')
		p.expect('\'')
		p.skipNewline()
	}
	start := p.i
	for !p.done() {
		if multiline && strings.HasPrefix(p.rest(), "'''") {
			out := p.in[start:p.i]
			p.advance(3)
			return out
		}
		if !multiline && p.peek() == '\'' {
			out := p.in[start:p.i]
			p.advance(1)
			return out
		}
		if !multiline && (p.peek() == '\n' || p.peek() == '\r') {
			p.fail("Literal strings cannot contain newlines")
		}
		p.advance(1)
	}
	p.fail("Unterminated literal string")
	return ""
}

// skipNewline skips the line break right after a multi-line string opens.
func (p *tomlParser) skipNewline() {
	if strings.HasPrefix(p.rest(), "\r\n") {
		p.advance(2)
	} else if p.peek() == '\n' {
		p.advance(1)
	}
}

func (p *tomlParser) parseArray() []Value {
	p.expect('[')
	out := []Value{}
	for {
		p.skipWhitespace()
		if p.peek() == ']' {
			p.advance(1)
			return out
		}
		out = append(out, p.parseValue())
		p.skipWhitespace()
		if p.peek() == ']' {
			p.advance(1)
			return out
		}
		p.expect(',')
	}
}

func (p *tomlParser) parseInlineTable() *Record {
	p.expect('{')
	out := &Record{}
	p.skipInlineWhitespace()
	if p.peek() == '}' {
		p.advance(1)
		return out
	}
	for {
		keys := p.parseKeyPath('=')
		p.skipInlineWhitespace()
		p.expect('=')
		p.skipInlineWhitespace()
		p.assign(out, keys, p.parseValue())
		p.skipInlineWhitespace()
		if p.peek() == '}' {
			p.advance(1)
			return out
		}
		p.expect(',')
		p.skipInlineWhitespace()
		if p.peek() == '}' {
			p.fail("Inline tables cannot end with a trailing comma")
		}
	}
}

// skipWhitespace skips white space, line breaks and comments, between
// statements as between the values of an array.
func (p *tomlParser) skipWhitespace() {
	for !p.done() {
		switch p.peek() {
		case ' ', '\t', '\r', '\n':
			p.advance(1)
		case '#':
			p.skipComment()
		default:
			return
		}
	}
}

func (p *tomlParser) skipInlineWhitespace() {
	for p.peek() == ' ' || p.peek() == '\t' {
		p.advance(1)
	}
}

func (p *tomlParser) finishStatement() {
	p.skipInlineWhitespace()
	if p.peek() == '#' {
		p.skipComment()
	}
	if !p.done() && p.peek() != '\n' && p.peek() != '\r' {
		p.fail("Expected the end of the line")
	}
}

func (p *tomlParser) skipComment() {
	for !p.done() && p.peek() != '\n' {
		p.advance(1)
	}
}

func (p *tomlParser) expect(c rune) {
	if p.peek() != c {
		p.fail("Expected '" + string(c) + "'")
	}
	p.advance(1)
}

// advance moves past `count` characters, ASCII but for a single one; as
// in JS, past the end it still counts columns.
func (p *tomlParser) advance(count int) {
	for range count {
		if p.done() {
			p.column++
			p.i++
			continue
		}
		r, n := decode(p.rest())
		if r == '\n' {
			p.line++
			p.column = 1
		} else if r >= 0x10000 {
			p.column += 2
		} else {
			p.column++
		}
		p.i += n
	}
}

// peek is the next character, -1 at the end.
func (p *tomlParser) peek() rune {
	if p.done() {
		return -1
	}
	r, _ := decode(p.rest())
	return r
}

func (p *tomlParser) done() bool { return p.i >= len(p.in) }

func (p *tomlParser) rest() string { return p.in[min(p.i, len(p.in)):] }

func (p *tomlParser) fail(message string) {
	panic(encodingError("SyntaxError: " + message + " at line " + strconv.Itoa(p.line) + ", column " + strconv.Itoa(p.column)))
}
