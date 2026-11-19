//go:build flickcli

package flickrt

import (
	"math"
	"regexp"
	"strconv"
	"strings"
)

// Effect's TOML parser (`encoding/Toml.js`): tables, arrays of tables,
// dotted keys, inline tables and arrays, strings, numbers and dates. An
// offset date-time is a JS Date, so its JSON is its ISO text in UTC; the
// local forms stay text. Lines and columns count as JS strings do.

type cliTomlParser struct {
	input        string
	i            int
	line, column int
	root         *Record
	current      *Record
	explicit     map[*Record]bool
}

func cliToml(input string) Value {
	root := &Record{}
	p := &cliTomlParser{input: strings.TrimPrefix(input, "﻿"), line: 1, column: 1, root: root, current: root,
		explicit: map[*Record]bool{}}
	for {
		p.skipDocumentSpace()
		if p.done() {
			return root
		}
		if p.peek() == '[' {
			p.header()
			continue
		}
		keys := p.keyPath('=')
		p.skipInlineSpace()
		p.expect('=')
		p.skipInlineSpace()
		p.assign(p.current, keys, p.value())
		p.finishStatement()
	}
}

func (p *cliTomlParser) done() bool { return p.i >= len(p.input) }

func (p *cliTomlParser) peek() byte {
	if p.done() {
		return 0
	}
	return p.input[p.i]
}

func (p *cliTomlParser) at(s string) bool { return strings.HasPrefix(p.input[p.i:], s) }

func (p *cliTomlParser) advance(count int) {
	for ; count > 0 && !p.done(); count-- {
		r, n := decode(p.input[p.i:])
		switch {
		case r == '\n':
			p.line++
			p.column = 1
		case r >= 0x10000:
			p.column += 2
		default:
			p.column++
		}
		p.i += n
	}
}

func (p *cliTomlParser) fail(message string) {
	panic(cliSyntax(message + " at line " + strconv.Itoa(p.line) + ", column " + strconv.Itoa(p.column)))
}

func (p *cliTomlParser) expect(c byte) {
	if p.peek() != c {
		p.fail("Expected '" + string(c) + "'")
	}
	p.advance(1)
}

func (p *cliTomlParser) header() {
	p.expect('[')
	array := p.peek() == '['
	if array {
		p.advance(1)
	}
	p.skipInlineSpace()
	path := p.keyPath(']')
	p.skipInlineSpace()
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

func (p *cliTomlParser) resolveTable(path []string, array bool) *Record {
	table := p.root
	for i, key := range path {
		at := table.index(key)
		if i == len(path)-1 && array {
			if at < 0 {
				table.put(key, &cliJSArray{})
				at = table.index(key)
			}
			a, ok := table.vals[at].(*cliJSArray)
			if !ok {
				p.fail("Cannot redefine existing key '" + strings.Join(path[:i+1], ".") + "'")
			}
			next := &Record{}
			a.items = append(a.items, next)
			return next
		}
		if at < 0 {
			table.put(key, &Record{})
			at = table.index(key)
		}
		v := table.vals[at]
		if a, ok := v.(*cliJSArray); ok {
			v = nil
			if len(a.items) > 0 {
				v = a.items[len(a.items)-1]
			}
		}
		next, ok := v.(*Record)
		if !ok {
			p.fail("Cannot redefine existing key '" + strings.Join(path[:i+1], ".") + "'")
		}
		table = next
	}
	return table
}

func (p *cliTomlParser) assign(target *Record, keys []string, v Value) {
	table := target
	for i, key := range keys[:len(keys)-1] {
		at := table.index(key)
		if at < 0 {
			child := &Record{}
			table.put(key, child)
			table = child
		} else if child, ok := table.vals[at].(*Record); ok {
			table = child
		} else {
			p.fail("Cannot redefine existing key '" + strings.Join(keys[:i+1], ".") + "'")
		}
	}
	key := keys[len(keys)-1]
	if table.index(key) >= 0 {
		p.fail("Cannot redefine existing key '" + strings.Join(keys, ".") + "'")
	}
	table.put(key, v)
}

func cliBareKey(c byte) bool {
	return c >= 'A' && c <= 'Z' || c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || c == '_' || c == '-'
}

func (p *cliTomlParser) keyPath(stop byte) []string {
	var keys []string
	for {
		p.skipInlineSpace()
		var key string
		switch p.peek() {
		case '"':
			key = p.basicString(false)
		case '\'':
			key = p.literalString(false)
		default:
			start := p.i
			for !p.done() && cliBareKey(p.peek()) {
				p.advance(1)
			}
			key = p.input[start:p.i]
		}
		if key == "" {
			p.fail("Expected a key")
		}
		keys = append(keys, key)
		p.skipInlineSpace()
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
	cliTomlSpacedDate = regexp.MustCompile(`^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})?)`)
	cliTomlOffsetDate = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:[Zz]|[+-]\d{2}:\d{2})$`)
	cliTomlLocalDate  = regexp.MustCompile(`^(?:\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(?:\.\d+)?|\d{4}-\d{2}-\d{2}|\d{2}:\d{2}:\d{2}(?:\.\d+)?)$`)
	cliTomlSpecial    = regexp.MustCompile(`^[+-]?(?:inf|nan)$`)
	cliTomlHex        = regexp.MustCompile(`^0x[0-9A-Fa-f](?:_?[0-9A-Fa-f])*$`)
	cliTomlOct        = regexp.MustCompile(`^0o[0-7](?:_?[0-7])*$`)
	cliTomlBin        = regexp.MustCompile(`^0b[01](?:_?[01])*$`)
	cliTomlInt        = regexp.MustCompile(`^[+-]?(?:0|[1-9](?:_?\d)*)$`)
	cliTomlFloat      = regexp.MustCompile(`^[+-]?(?:(?:0|[1-9](?:_?\d)*)\.\d(?:_?\d)*(?:[eE][+-]?\d(?:_?\d)*)?|(?:0|[1-9](?:_?\d)*)[eE][+-]?\d(?:_?\d)*)$`)
)

func (p *cliTomlParser) value() Value {
	switch p.peek() {
	case '"':
		return p.basicString(p.at(`"""`))
	case '\'':
		return p.literalString(p.at("'''"))
	case '[':
		return p.array()
	case '{':
		return p.inlineTable()
	}
	if m := cliTomlSpacedDate.FindStringSubmatch(p.input[p.i:]); m != nil {
		p.advance(len(m[0]))
		return p.date(m[1] + "T" + m[2])
	}
	start := p.i
	for !p.done() {
		r, _ := decode(p.input[p.i:])
		if isJSSpace(r) || strings.ContainsRune(",#]}", r) {
			break
		}
		p.advance(1)
	}
	token := p.input[start:p.i]
	switch token {
	case "true":
		return true
	case "false":
		return false
	case "":
		p.fail("Expected a value")
	}
	if d := p.date(token); d != nil {
		return d
	}
	if n, ok := cliTomlNumber(token); ok {
		return n
	}
	p.fail("Invalid value '" + token + "'")
	return nil
}

// date is an offset date-time as its JSON, a local one as its text; nil
// for a token that is no date.
func (p *cliTomlParser) date(token string) Value {
	normal := strings.Replace(token, "t", "T", 1)
	if cliTomlOffsetDate.MatchString(token) {
		t := parseJSDate(strings.Replace(normal, "z", "Z", 1))
		if math.IsNaN(t) {
			p.fail("Invalid date-time '" + token + "'")
		}
		return (&date{t}).isoString()
	}
	if cliTomlLocalDate.MatchString(token) {
		return normal
	}
	return nil
}

func cliTomlNumber(token string) (Value, bool) {
	normalized := strings.ReplaceAll(token, "_", "")
	switch {
	case cliTomlSpecial.MatchString(token):
		if strings.HasSuffix(normalized, "nan") {
			return math.NaN(), true
		}
		if normalized[0] == '-' {
			return math.Inf(-1), true
		}
		return math.Inf(1), true
	case cliTomlHex.MatchString(token):
		return cliParseInt(normalized[2:], 16), true
	case cliTomlOct.MatchString(token):
		return cliParseInt(normalized[2:], 8), true
	case cliTomlBin.MatchString(token):
		return cliParseInt(normalized[2:], 2), true
	case cliTomlInt.MatchString(token) || cliTomlFloat.MatchString(token):
		n, _ := strconv.ParseFloat(normalized, 64)
		return n, true
	}
	return nil, false
}

var cliTomlEscapes = map[byte]string{'b': "\b", 't': "\t", 'n': "\n", 'f': "\f", 'r': "\r", '"': "\"", '\\': "\\"}

func (p *cliTomlParser) openString(quote byte, multiline bool) {
	p.expect(quote)
	if multiline {
		p.expect(quote)
		p.expect(quote)
		if p.at("\r\n") {
			p.advance(2)
		} else if p.peek() == '\n' {
			p.advance(1)
		}
	}
}

func (p *cliTomlParser) basicString(multiline bool) string {
	p.openString('"', multiline)
	var out []byte
	for !p.done() {
		if multiline && p.at(`"""`) {
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
			_, n := decode(p.input[p.i:])
			out = append(out, p.input[p.i:p.i+n]...)
			p.advance(1)
			continue
		}
		p.advance(1)
		if multiline && strings.ContainsRune(" \t\r\n", rune(p.peek())) && !p.done() {
			for p.peek() == ' ' || p.peek() == '\t' {
				p.advance(1)
			}
			if p.peek() != '\n' && p.peek() != '\r' {
				p.fail("Invalid multiline string continuation")
			}
			for !p.done() && strings.ContainsRune(" \t\r\n", rune(p.peek())) {
				p.advance(1)
			}
			continue
		}
		if p.done() {
			p.fail(`Invalid escape '\'`)
		}
		e, _ := decode(p.input[p.i:])
		p.advance(1)
		if s, ok := cliTomlEscapes[byte(e)]; ok && e < 0x80 {
			out = append(out, s...)
			continue
		}
		if e == 'u' || e == 'U' {
			n := 4
			if e == 'U' {
				n = 8
			}
			hex := p.input[p.i:min(p.i+n, len(p.input))]
			code, err := strconv.ParseUint(hex, 16, 32)
			if len(hex) != n || err != nil || strings.ContainsAny(hex, "+-") {
				p.fail("Invalid unicode escape")
			}
			if code > 0x10FFFF {
				panic(cliJSError("RangeError: Invalid code point " + strconv.FormatUint(code, 10)))
			}
			out = encodeRune(out, rune(code))
			p.advance(n)
			continue
		}
		p.fail(`Invalid escape '\` + string(e) + "'")
	}
	p.fail("Unterminated basic string")
	return ""
}

func (p *cliTomlParser) literalString(multiline bool) string {
	p.openString('\'', multiline)
	start := p.i
	for !p.done() {
		if multiline && p.at("'''") {
			out := p.input[start:p.i]
			p.advance(3)
			return out
		}
		if !multiline && p.peek() == '\'' {
			out := p.input[start:p.i]
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

func (p *cliTomlParser) array() Value {
	p.expect('[')
	out := &cliJSArray{items: []Value{}}
	for {
		p.skipDocumentSpace()
		if p.peek() == ']' {
			p.advance(1)
			return out
		}
		out.items = append(out.items, p.value())
		p.skipDocumentSpace()
		if p.peek() == ']' {
			p.advance(1)
			return out
		}
		p.expect(',')
	}
}

func (p *cliTomlParser) inlineTable() Value {
	p.expect('{')
	out := &Record{}
	p.skipInlineSpace()
	if p.peek() == '}' {
		p.advance(1)
		return out
	}
	for {
		keys := p.keyPath('=')
		p.skipInlineSpace()
		p.expect('=')
		p.skipInlineSpace()
		p.assign(out, keys, p.value())
		p.skipInlineSpace()
		if p.peek() == '}' {
			p.advance(1)
			return out
		}
		p.expect(',')
		p.skipInlineSpace()
		if p.peek() == '}' {
			p.fail("Inline tables cannot end with a trailing comma")
		}
	}
}

// skipDocumentSpace skips white space, line breaks and comments, as
// between statements and inside arrays.
func (p *cliTomlParser) skipDocumentSpace() {
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

func (p *cliTomlParser) skipInlineSpace() {
	for p.peek() == ' ' || p.peek() == '\t' {
		p.advance(1)
	}
}

func (p *cliTomlParser) finishStatement() {
	p.skipInlineSpace()
	if p.peek() == '#' {
		p.skipComment()
	}
	if !p.done() && p.peek() != '\n' && p.peek() != '\r' {
		p.fail("Expected the end of the line")
	}
}

func (p *cliTomlParser) skipComment() {
	for !p.done() && p.peek() != '\n' {
		p.advance(1)
	}
}
