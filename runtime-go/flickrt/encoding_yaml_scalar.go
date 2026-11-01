package flickrt

import (
	"math"
	"regexp"
	"strconv"
	"strings"
)

// The scalars, indicators and flow collections of Effect's encoding/Yaml
// (based on `yaml` 2.9.0); its block structure is in encoding_yaml.go.

func isSpaceAt(s string, i int) bool {
	if i >= len(s) {
		return false
	}
	r, _ := decode(s[i:])
	return isJSSpace(r)
}

// closingQuote is the index of the quote closing a scalar opened before
// `start`, -1 for none; a quote and a backslash are ASCII, so bytes do.
func closingQuote(input string, quote byte, start int) int {
	for i := start; i < len(input); i++ {
		c := input[i]
		if quote == '"' && c == '\\' {
			i++
		} else if c == quote {
			if quote == '\'' && i+1 < len(input) && input[i+1] == quote {
				i++
			} else {
				return i
			}
		}
	}
	return -1
}

// findIndicator is the index of the comment `#` or the mapping `:` of a
// line, outside quotes and flow collections; -1 for none.
func findIndicator(input string, indicator byte) int {
	depth, tokenStart := 0, true
	for i := 0; i < len(input); {
		c, n := decode(input[i:])
		switch {
		case tokenStart && (c == '\'' || c == '"'):
			i = closingQuote(input, byte(c), i+1)
			if i == -1 {
				return -1
			}
			tokenStart = false
		case tokenStart && c == '&':
			for i+n < len(input) {
				next, m := decode(input[i+n:])
				if isJSSpace(next) || strings.ContainsRune(",[]{}", next) {
					break
				}
				i += n
				n = m
			}
		case c == '#' && (i == 0 || isJSSpace(lastCodePoint(input[:i]))):
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
		case c == ':' && (depth > 0 || i+n == len(input) || isSpaceAt(input, i+n)):
			if depth == 0 && indicator == ':' {
				return i
			}
			tokenStart = true
		case tokenStart && c == '-' && isSpaceAt(input, i+n):
		case !isJSSpace(c):
			tokenStart = false
		}
		_, n = decode(input[i:])
		i += n
	}
	return -1
}

// lastCodePoint is the last code point of `s`.
func lastCodePoint(s string) rune {
	i := len(s) - 1
	for i > 0 && s[i]&0xC0 == 0x80 {
		i--
	}
	r, _ := decode(s[i:])
	return r
}

func stripComment(input string) string {
	if i := findIndicator(input, '#'); i != -1 {
		return strings.TrimRightFunc(input[:i], isJSSpace)
	}
	return input
}

func isSequenceEntry(text string) bool { return text == "-" || strings.HasPrefix(text, "- ") }

func foldLineBreak(blankLines int) string {
	if blankLines == 0 {
		return " "
	}
	return strings.Repeat("\n", blankLines)
}

func endsWithEscape(text string) bool {
	return (len(text)-len(strings.TrimRight(text, `\`)))%2 == 1
}

// trimFoldedEnd drops the trailing blanks of a folded line, but for an
// escaped one, which is content.
func trimFoldedEnd(line string, quote byte) string {
	at := len(strings.TrimRight(line, " \t"))
	if at == len(line) {
		return line
	}
	if quote == '"' && endsWithEscape(line[:at]) {
		at++
	}
	return line[:at]
}

// hasValueColon is /:(?:\s|$)/: a `:` that would start a mapping value.
func hasValueColon(s string) bool {
	for i := 0; i < len(s); i++ {
		if s[i] == ':' && (i+1 == len(s) || isSpaceAt(s, i+1)) {
			return true
		}
	}
	return false
}

var yamlEscapes = map[rune]string{
	'0': "\x00", 'a': "\x07", 'b': "\b", 't': "\t", 'n': "\n", 'v': "\v", 'f': "\f", 'r': "\r", 'e': "\x1b",
	' ': " ", '"': "\"", '/': "/", '\\': "\\", 'N': "\u0085", '_': "\u00a0", 'L': "\u2028", 'P': "\u2029",
}

func parseDoubleQuoted(input string) string {
	if !strings.HasSuffix(input, `"`) || len(input) < 2 {
		panic(encodingError("SyntaxError: Unterminated double-quoted YAML string"))
	}
	var out []byte
	for i := 1; i < len(input)-1; {
		c, n := decode(input[i:])
		if c != '\\' {
			out = append(out, input[i:i+n]...)
			i += n
			continue
		}
		i++
		escape, n := decode(input[i:])
		i += n
		if e, ok := yamlEscapes[escape]; ok {
			out = append(out, e...)
			continue
		}
		if length := hexEscapeLength(escape); length > 0 {
			r, ok := hexCodePoint(input[i:], length)
			if !ok {
				panic(encodingError("SyntaxError: Invalid unicode escape in YAML string"))
			}
			out = appendCodePoint(out, r)
			i += length
			continue
		}
		panic(encodingError("SyntaxError: Invalid YAML escape '\\" + input[i-n:i] + "'"))
	}
	return string(out)
}

func hexEscapeLength(escape rune) int {
	switch escape {
	case 'x':
		return 2
	case 'u':
		return 4
	case 'U':
		return 8
	}
	return 0
}

// asciiLower lowers A to Z only, as a JS /i pattern without the u flag
// matches ASCII letters.
func asciiLower(s string) string {
	return strings.Map(func(r rune) rune {
		if r >= 'A' && r <= 'Z' {
			return r + 'a' - 'A'
		}
		return r
	}, s)
}

var (
	yamlHex     = regexp.MustCompile(`^[+-]?0x[0-9a-f]+$`)
	yamlOctal   = regexp.MustCompile(`^[+-]?0o[0-7]+$`)
	yamlInteger = regexp.MustCompile(`^[+-]?(?:0|[1-9]\d*)$`)
	yamlFloat   = regexp.MustCompile(`^[+-]?(?:(?:0|[1-9]\d*)?\.\d+|(?:0|[1-9]\d*)\.?\d*[eE][+-]?\d+)$`)
)

// parseYamlScalar is a scalar of YAML 1.2's core schema: null, a boolean,
// a number or a string.
func parseYamlScalar(input string) Value {
	value := strings.TrimFunc(input, isJSSpace)
	switch {
	case value == "":
		return JsonNull
	case value[0] == '"' && value[len(value)-1] == '"':
		return parseDoubleQuoted(value)
	case value[0] == '\'' && value[len(value)-1] == '\'':
		if len(value) < 2 {
			return ""
		}
		return strings.ReplaceAll(value[1:len(value)-1], "''", "'")
	}
	switch lower := asciiLower(value); lower {
	case "null", "~":
		return JsonNull
	case "true":
		return true
	case "false":
		return false
	case ".inf", "+.inf":
		return math.Inf(1)
	case "-.inf":
		return math.Inf(-1)
	case ".nan":
		return math.NaN()
	}
	normalized := strings.ReplaceAll(value, "_", "")
	sign := 1.0
	if strings.HasPrefix(normalized, "-") {
		sign = -1
	}
	lower := asciiLower(normalized)
	switch {
	case yamlHex.MatchString(lower):
		return sign * parseIntRadix(lower[strings.Index(lower, "x")+1:], 16)
	case yamlOctal.MatchString(lower):
		return sign * parseIntRadix(lower[strings.Index(lower, "o")+1:], 8)
	case yamlInteger.MatchString(normalized) || yamlFloat.MatchString(normalized):
		n, _ := strconv.ParseFloat(normalized, 64)
		return n
	}
	return value
}

// parseYamlKey is a mapping key: `String` of its scalar, null empty.
func parseYamlKey(input string) string {
	v := parseYamlScalar(input)
	if v == JsonNull {
		return ""
	}
	return stringOfJSON(v)
}

// yamlFlow is Effect's FlowParser of a one-line `[…]` or `{…}`.
type yamlFlow struct {
	in      string
	i       int
	anchors map[string]Value
}

func (p *yamlFlow) parse() Value {
	v := p.parseValue()
	p.skipWhitespace()
	if p.i != len(p.in) {
		p.fail("Unexpected flow collection content")
	}
	return v
}

func (p *yamlFlow) parseValue() Value {
	p.skipWhitespace()
	switch c := p.peek(); c {
	case '[':
		return p.parseSequence()
	case '{':
		return p.parseMapping()
	case '"', '\'':
		return parseYamlScalar(p.readQuoted(byte(c)))
	case '*':
		p.i++
		name := p.readUntil(func(r rune) bool { return isJSSpace(r) || strings.ContainsRune(",]}", r) })
		v, ok := p.anchors[name]
		if !ok {
			p.fail("Unknown alias '*" + name + "'")
		}
		return v
	}
	return parseYamlScalar(p.readUntil(func(r rune) bool { return strings.ContainsRune(",]}", r) }))
}

func (p *yamlFlow) parseSequence() []Value {
	p.expect('[')
	out := []Value{}
	p.skipWhitespace()
	if p.peek() == ']' {
		p.i++
		return out
	}
	for {
		out = append(out, p.parseValue())
		p.skipWhitespace()
		if p.peek() == ']' {
			p.i++
			return out
		}
		p.expect(',')
		p.skipWhitespace()
		if p.peek() == ']' {
			p.i++
			return out
		}
	}
}

func (p *yamlFlow) parseMapping() *Record {
	p.expect('{')
	out := &Record{}
	p.skipWhitespace()
	if p.peek() == '}' {
		p.i++
		return out
	}
	for {
		key := p.parseKey()
		p.skipWhitespace()
		p.expect(':')
		if has(out, key) {
			p.fail("Duplicate key '" + key + "'")
		}
		out.put(key, p.parseValue())
		p.skipWhitespace()
		if p.peek() == '}' {
			p.i++
			return out
		}
		p.expect(',')
		p.skipWhitespace()
		if p.peek() == '}' {
			p.i++
			return out
		}
	}
}

func (p *yamlFlow) parseKey() string {
	p.skipWhitespace()
	if c := p.peek(); c == '"' || c == '\'' {
		return parseYamlKey(p.readQuoted(byte(c)))
	}
	return strings.TrimFunc(p.readUntil(func(r rune) bool { return r == ':' }), isJSSpace)
}

func (p *yamlFlow) readQuoted(quote byte) string {
	start := p.i
	end := closingQuote(p.in, quote, start+1)
	if end == -1 {
		p.fail("Unterminated quoted scalar")
	}
	p.i = end + 1
	return p.in[start:p.i]
}

func (p *yamlFlow) readUntil(stop func(rune) bool) string {
	start := p.i
	for p.i < len(p.in) {
		r, n := decode(p.in[p.i:])
		if stop(r) {
			break
		}
		p.i += n
	}
	return p.in[start:p.i]
}

func (p *yamlFlow) skipWhitespace() {
	for isSpaceAt(p.in, p.i) {
		_, n := decode(p.in[p.i:])
		p.i += n
	}
}

func (p *yamlFlow) expect(c rune) {
	if p.peek() != c {
		p.fail("Expected '" + string(c) + "'")
	}
	p.i++
}

func (p *yamlFlow) peek() rune {
	if p.i >= len(p.in) {
		return -1
	}
	r, _ := decode(p.in[p.i:])
	return r
}

func (p *yamlFlow) fail(message string) {
	panic(encodingError("SyntaxError: " + message + " at flow offset " + strconv.Itoa(utf16Len(p.in[:p.i]))))
}
