//go:build flickcli

package flickrt

import (
	"math"
	"regexp"
	"strconv"
	"strings"
	"unicode/utf8"
)

// Effect's YAML parser (`encoding/Yaml.js`): block mappings and sequences,
// flow collections, quoted, plain and block scalars, anchors and aliases,
// one document. Positions are those of JS strings.

func cliRuneAt(s string, i int) rune {
	if i < 0 || i >= len(s) {
		return -1
	}
	r, _ := decode(s[i:])
	return r
}

func cliSpaceAt(s string, i int) bool { r := cliRuneAt(s, i); return r >= 0 && isJSSpace(r) }

func cliNext(s string, i int) int {
	_, n := decode(s[i:])
	return i + n
}

func cliPrevSpace(s string, i int) bool {
	r, _ := utf8.DecodeLastRuneInString(s[:i])
	return isJSSpace(r)
}

func cliTrim(s string) string      { return strings.TrimFunc(s, isJSSpace) }
func cliTrimEnd(s string) string   { return strings.TrimRightFunc(s, isJSSpace) }
func cliTrimStart(s string) string { return strings.TrimLeftFunc(s, isJSSpace) }

func cliClosingQuote(input string, quote byte, start int) int {
	for i := start; i < len(input); i++ {
		switch c := input[i]; {
		case quote == '"' && c == '\\':
			i++
		case c == quote:
			if quote == '\'' && i+1 < len(input) && input[i+1] == quote {
				i++
			} else {
				return i
			}
		}
	}
	return -1
}

// cliFindIndicator is where `#` starts a comment or `:` separates a key,
// outside quotes and flow collections; -1 for none.
func cliFindIndicator(input string, indicator byte) int {
	depth, tokenStart := 0, true
	for i := 0; i < len(input); i = cliNext(input, i) {
		c := input[i]
		switch {
		case tokenStart && (c == '\'' || c == '"'):
			if i = cliClosingQuote(input, c, i+1); i == -1 {
				return -1
			}
			tokenStart = false
		case tokenStart && c == '&':
			for i+1 < len(input) && !cliSpaceAt(input, i+1) && !strings.ContainsRune(",[]{}", cliRuneAt(input, i+1)) {
				i = cliNext(input, i+1) - 1
			}
		case c == '#' && (i == 0 || cliPrevSpace(input, i)):
			if indicator == '#' {
				return i
			}
			return -1
		case tokenStart && (c == '[' || c == '{'):
			depth++
		case depth > 0 && (c == ']' || c == '}'):
			depth--
			tokenStart = false
		case depth > 0 && c == ',':
			tokenStart = true
		case c == ':' && (depth > 0 || i+1 >= len(input) || cliSpaceAt(input, i+1)):
			if depth == 0 && indicator == ':' {
				return i
			}
			tokenStart = true
		case tokenStart && c == '-' && cliSpaceAt(input, i+1):
		case !cliSpaceAt(input, i):
			tokenStart = false
		}
	}
	return -1
}

func cliStripComment(input string) string {
	if i := cliFindIndicator(input, '#'); i >= 0 {
		return cliTrimEnd(input[:i])
	}
	return input
}

func cliSequenceEntry(text string) bool { return text == "-" || strings.HasPrefix(text, "- ") }

func cliFold(blank int) string {
	if blank == 0 {
		return " "
	}
	return strings.Repeat("\n", blank)
}

func cliEndsWithEscape(text string) bool {
	n := 0
	for i := len(text) - 1; i >= 0 && text[i] == '\\'; i-- {
		n++
	}
	return n%2 == 1
}

var cliYamlTrailing = regexp.MustCompile(`[ \t]+$`)

func cliTrimFoldedEnd(line string, quote byte) string {
	m := cliYamlTrailing.FindStringIndex(line)
	if m == nil {
		return line
	}
	if quote == '"' && cliEndsWithEscape(line[:m[0]]) {
		return line[:m[0]+1]
	}
	return line[:m[0]]
}

var cliYamlEscapes = map[byte]string{'0': "\x00", 'a': "\x07", 'b': "\b", 't': "\t", 'n': "\n", 'v': "\v", 'f': "\f",
	'r': "\r", 'e': "\x1b", ' ': " ", '"': "\"", '/': "/", '\\': "\\", 'N': "\u0085", '_': "\u00a0", 'L': "\u2028", 'P': "\u2029"}

func cliDoubleQuoted(input string) string {
	if len(input) < 2 || !strings.HasSuffix(input, `"`) {
		panic(cliSyntax("Unterminated double-quoted YAML string"))
	}
	var out []byte
	for i := 1; i < len(input)-1; i++ {
		if input[i] != '\\' {
			out = append(out, input[i])
			continue
		}
		i++
		e := input[i]
		if s, ok := cliYamlEscapes[e]; ok {
			out = append(out, s...)
			continue
		}
		if e == 'x' || e == 'u' || e == 'U' {
			n := map[byte]int{'x': 2, 'u': 4, 'U': 8}[e]
			hex := input[i+1 : min(i+1+n, len(input))]
			code, err := strconv.ParseUint(hex, 16, 32)
			if len(hex) != n || err != nil || strings.ContainsAny(hex, "+-") {
				panic(cliSyntax("Invalid unicode escape in YAML string"))
			}
			if code > 0x10FFFF {
				panic(cliJSError("RangeError: Invalid code point " + strconv.FormatUint(code, 10)))
			}
			out = encodeRune(out, rune(code))
			i += n
			continue
		}
		r, _ := decode(input[i:])
		panic(cliSyntax(`Invalid YAML escape '\` + string(r) + "'"))
	}
	return string(out)
}

func cliJSError(text string) cliSyntaxError { return cliSyntaxError{text} }

var (
	cliYamlNull  = regexp.MustCompile(`(?i)^(?:null|~)$`)
	cliYamlInf   = regexp.MustCompile(`(?i)^[+-]?\.inf$`)
	cliYamlNaN   = regexp.MustCompile(`(?i)^\.nan$`)
	cliYamlHex   = regexp.MustCompile(`(?i)^[+-]?0x[0-9a-f]+$`)
	cliYamlOct   = regexp.MustCompile(`(?i)^[+-]?0o[0-7]+$`)
	cliYamlInt   = regexp.MustCompile(`^[+-]?(?:0|[1-9]\d*)$`)
	cliYamlFloat = regexp.MustCompile(`^[+-]?(?:(?:0|[1-9]\d*)?\.\d+|(?:0|[1-9]\d*)\.?\d*[eE][+-]?\d+)$`)
)

// cliParseInt is JS's `Number.parseInt` of digits in a base.
func cliParseInt(digits string, base int) float64 {
	n := 0.0
	for _, c := range strings.ToLower(digits) {
		d := strings.IndexRune("0123456789abcdef", c)
		n = n*float64(base) + float64(d)
	}
	return n
}

func cliScalar(input string) Value {
	value := cliTrim(input)
	switch {
	case value == "":
		return JsonNull
	case len(value) >= 2 && value[0] == '"' && value[len(value)-1] == '"' || value == `"`:
		return cliDoubleQuoted(value)
	case len(value) >= 2 && value[0] == '\'' && value[len(value)-1] == '\'':
		return strings.ReplaceAll(value[1:len(value)-1], "''", "'")
	case value == "'":
		return ""
	case cliYamlNull.MatchString(value):
		return JsonNull
	case cliASCIIFold(value) == "true":
		return true
	case cliASCIIFold(value) == "false":
		return false
	case cliYamlInf.MatchString(value):
		if value[0] == '-' {
			return math.Inf(-1)
		}
		return math.Inf(1)
	case cliYamlNaN.MatchString(value):
		return math.NaN()
	}
	normalized := strings.ReplaceAll(value, "_", "")
	sign := 1.0
	if normalized[0] == '-' {
		sign = -1
	}
	unsigned := strings.TrimLeft(normalized, "+-")
	switch {
	case cliYamlHex.MatchString(normalized):
		return sign * cliParseInt(unsigned[2:], 16)
	case cliYamlOct.MatchString(normalized):
		return sign * cliParseInt(unsigned[2:], 8)
	case cliYamlInt.MatchString(normalized) || cliYamlFloat.MatchString(normalized):
		n, _ := strconv.ParseFloat(normalized, 64)
		return n
	}
	return value
}

// cliASCIIFold is the text with ASCII letters lower-cased, as a JS
// regular expression with `i` compares them.
func cliASCIIFold(s string) string {
	b := []byte(s)
	for i, c := range b {
		if c >= 'A' && c <= 'Z' {
			b[i] = c + 32
		}
	}
	return string(b)
}

func cliYamlKey(input string) string {
	v := cliScalar(input)
	if v == JsonNull {
		return ""
	}
	return cliJSString(v)
}

// cliFlow reads a flow collection: `[a, b]`, `{a: b}`.
type cliFlow struct {
	input   string
	anchors map[string]Value
	i       int
}

func (f *cliFlow) fail(message string) {
	panic(cliSyntax(message + " at flow offset " + strconv.Itoa(len(utf16Units(f.input[:f.i])))))
}

func (f *cliFlow) peek() byte {
	if f.i < len(f.input) {
		return f.input[f.i]
	}
	return 0
}

func (f *cliFlow) skipSpace() {
	for cliSpaceAt(f.input, f.i) {
		f.i = cliNext(f.input, f.i)
	}
}

func (f *cliFlow) expect(c byte) {
	if f.peek() != c {
		f.fail("Expected '" + string(c) + "'")
	}
	f.i++
}

func (f *cliFlow) readUntil(stop func(rune) bool) string {
	start := f.i
	for f.i < len(f.input) && !stop(cliRuneAt(f.input, f.i)) {
		f.i = cliNext(f.input, f.i)
	}
	return f.input[start:f.i]
}

func (f *cliFlow) readQuoted(quote byte) string {
	start := f.i
	end := cliClosingQuote(f.input, quote, start+1)
	if end == -1 {
		f.fail("Unterminated quoted scalar")
	}
	f.i = end + 1
	return f.input[start:f.i]
}

func (f *cliFlow) parse() Value {
	v := f.value()
	f.skipSpace()
	if f.i != len(f.input) {
		f.fail("Unexpected flow collection content")
	}
	return v
}

func (f *cliFlow) value() Value {
	f.skipSpace()
	switch c := f.peek(); c {
	case '[':
		return f.sequence()
	case '{':
		return f.mapping()
	case '"', '\'':
		return cliScalar(f.readQuoted(c))
	case '*':
		f.i++
		name := f.readUntil(func(r rune) bool { return r == ',' || r == ']' || r == '}' || isJSSpace(r) })
		v, ok := f.anchors[name]
		if !ok {
			f.fail("Unknown alias '*" + name + "'")
		}
		return v
	}
	return cliScalar(cliTrim(f.readUntil(func(r rune) bool { return r == ',' || r == ']' || r == '}' })))
}

func (f *cliFlow) sequence() Value {
	f.expect('[')
	out := []Value{}
	f.skipSpace()
	if f.peek() == ']' {
		f.i++
		return out
	}
	for {
		out = append(out, f.value())
		f.skipSpace()
		if f.peek() == ']' {
			f.i++
			return out
		}
		f.expect(',')
		f.skipSpace()
		if f.peek() == ']' {
			f.i++
			return out
		}
	}
}

func (f *cliFlow) mapping() Value {
	f.expect('{')
	out := &Record{}
	f.skipSpace()
	if f.peek() == '}' {
		f.i++
		return out
	}
	for {
		f.skipSpace()
		var key string
		if c := f.peek(); c == '"' || c == '\'' {
			key = cliYamlKey(f.readQuoted(c))
		} else {
			key = cliTrim(f.readUntil(func(r rune) bool { return r == ':' }))
		}
		f.skipSpace()
		f.expect(':')
		if out.index(key) >= 0 {
			f.fail("Duplicate key '" + key + "'")
		}
		out.put(key, f.value())
		f.skipSpace()
		if f.peek() == '}' {
			f.i++
			return out
		}
		f.expect(',')
		f.skipSpace()
		if f.peek() == '}' {
			f.i++
			return out
		}
	}
}
