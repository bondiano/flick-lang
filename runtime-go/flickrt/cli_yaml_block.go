//go:build flickcli

package flickrt

import (
	"regexp"
	"strconv"
	"strings"
)

// The block structure of Effect's YAML parser: indented lines of mappings,
// sequences and scalars.

type cliYamlLine struct {
	raw, text      string
	indent, number int
}

type cliYamlParser struct {
	lines   []cliYamlLine
	i       int
	started bool
	anchors map[string]Value
}

var cliYamlLeadingSpaces = regexp.MustCompile(`^ *`)

// cliYaml is Effect's `Yaml.parse` of one document.
func cliYaml(input string) Value {
	source := strings.TrimPrefix(input, "\uFEFF")
	source = strings.ReplaceAll(strings.ReplaceAll(source, "\r\n", "\n"), "\r", "\n")
	var lines []cliYamlLine
	for n, raw := range strings.Split(source, "\n") {
		indent := len(cliYamlLeadingSpaces.FindString(raw))
		if strings.Contains(raw[:min(indent+1, len(raw))], "\t") {
			panic(cliSyntax("Tabs cannot be used for YAML indentation at line " + strconv.Itoa(n+1)))
		}
		lines = append(lines, cliYamlLine{raw, cliStripComment(raw[indent:]), indent, n + 1})
	}
	p := &cliYamlParser{lines: lines, anchors: map[string]Value{}}
	return p.parse()
}

func (p *cliYamlParser) line() *cliYamlLine {
	if p.i < len(p.lines) {
		return &p.lines[p.i]
	}
	return nil
}

func (p *cliYamlParser) fail(l *cliYamlLine, message string) {
	panic(cliSyntax(message + " at line " + strconv.Itoa(l.number)))
}

func (p *cliYamlParser) parse() Value {
	p.skipIgnored()
	if p.i >= len(p.lines) {
		return JsonNull
	}
	p.started = true
	v := p.node(p.lines[p.i].indent, p.lines[p.i].indent-1)
	p.skipIgnored()
	if l := p.line(); l != nil {
		p.fail(l, "Unexpected content")
	}
	return v
}

func (p *cliYamlParser) node(indent, parent int) Value {
	p.skipIgnored()
	l := p.line()
	if l == nil {
		return JsonNull
	}
	if l.indent != indent {
		p.fail(l, "Expected indentation of "+strconv.Itoa(indent)+" spaces")
	}
	if cliSequenceEntry(l.text) {
		return p.sequence(indent)
	}
	if cliFindIndicator(l.text, ':') != -1 {
		return p.mapping(indent, &Record{})
	}
	p.i++
	return p.nodeValue(l.text, parent, false)
}

func (p *cliYamlParser) mapping(indent int, out *Record) Value {
	for {
		p.skipIgnored()
		l := p.line()
		if l == nil || l.indent < indent {
			return out
		}
		if l.indent > indent {
			p.fail(l, "Unexpected indentation of "+strconv.Itoa(l.indent)+" spaces")
		}
		sep := cliFindIndicator(l.text, ':')
		if sep == -1 {
			return out
		}
		p.i++
		p.entry(out, l.text, sep, indent, l)
	}
}

func (p *cliYamlParser) entry(out *Record, text string, sep, indent int, l *cliYamlLine) {
	key := cliYamlKey(cliTrim(text[:sep]))
	v := p.nodeValue(cliTrimStart(text[sep+1:]), indent, true)
	if out.index(key) >= 0 {
		p.fail(l, "Duplicate key '"+key+"'")
	}
	out.put(key, v)
}

func (p *cliYamlParser) sequence(indent int) Value {
	out := []Value{}
	for {
		p.skipIgnored()
		l := p.line()
		if l == nil || l.indent < indent {
			return out
		}
		if l.indent > indent {
			p.fail(l, "Unexpected indentation of "+strconv.Itoa(l.indent)+" spaces")
		}
		if !cliSequenceEntry(l.text) {
			return out
		}
		p.i++
		item := cliTrimStart(l.text[1:])
		if cliSequenceEntry(item) {
			p.fail(l, "Compact nested block sequences are not supported")
		}
		if sep := cliFindIndicator(item, ':'); sep == -1 {
			out = append(out, p.nodeValue(item, indent, false))
		} else {
			m := &Record{}
			p.entry(m, item, sep, indent+2, l)
			out = append(out, p.mapping(indent+2, m))
		}
	}
}

var (
	cliYamlAnchor = regexp.MustCompile(`^&([^\t\n\v\f\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF},\[\]{}]+)(?:` + cliJSSpace + `+([\s\S]*))?$`)
	cliYamlBlock  = regexp.MustCompile(`^[|>](?:[1-9]?[+-]?|[+-]?[1-9]?)$`)
)

func (p *cliYamlParser) nodeValue(raw string, parent int, indentless bool) Value {
	m := cliYamlAnchor.FindStringSubmatch(raw)
	if m == nil {
		return p.value(raw, parent, indentless)
	}
	v := p.value(m[2], parent, indentless)
	p.anchors[m[1]] = v
	return v
}

func (p *cliYamlParser) value(value string, parent int, indentless bool) Value {
	text := cliTrimEnd(value)
	switch {
	case text == "":
		p.skipIgnored()
		next := p.line()
		switch {
		case next == nil:
			return JsonNull
		case next.indent > parent:
			return p.node(next.indent, parent)
		case indentless && next.indent == parent && cliSequenceEntry(next.text):
			return p.sequence(parent)
		}
		return JsonNull
	case cliYamlBlock.MatchString(text):
		return p.blockScalar(text, parent)
	case value[0] == '"' || value[0] == '\'':
		return p.quotedValue(value, parent)
	case text[0] == '*':
		name := cliTrim(text[1:])
		v, ok := p.anchors[name]
		if !ok {
			panic(cliSyntax("Unknown YAML alias '*" + name + "'"))
		}
		return v
	case text[0] == '[' || text[0] == '{':
		return (&cliFlow{input: text, anchors: p.anchors}).parse()
	}
	return p.plainValue(text, parent)
}

var (
	cliYamlColon      = regexp.MustCompile(`:(?:` + cliJSSpace + `|$)`)
	cliYamlComment    = regexp.MustCompile(`(?:^|[ \t])#`)
	cliYamlDocumentAt = regexp.MustCompile(`^(?:---|\.\.\.)(?:` + cliJSSpace + `|$)`)
)

func (p *cliYamlParser) plainValue(value string, parent int) Value {
	first := &p.lines[p.i-1]
	if cliYamlColon.MatchString(value) {
		p.fail(first, "Invalid colon in plain scalar")
	}
	out := value
	terminated := cliFindIndicator(first.raw, '#') != -1
	for !terminated {
		blank := p.skipBlankLines()
		next := p.line()
		if next == nil || next.indent <= parent {
			break
		}
		content := next.raw[next.indent:]
		comment := -1
		if m := cliYamlComment.FindStringIndex(content); m != nil {
			comment = m[0]
		}
		text := content
		if comment != -1 {
			text = content[:comment]
		}
		text = cliTrimEnd(text)
		if text == "" || next.indent == 0 && cliYamlDocumentAt.MatchString(text) {
			break
		}
		if cliYamlColon.MatchString(text) {
			p.fail(next, "Invalid colon in plain scalar")
		}
		out += cliFold(blank) + text
		p.i++
		terminated = comment != -1
	}
	return cliScalar(out)
}

var cliYamlCommentAfter = regexp.MustCompile(`^[ \t]+#`)

func (p *cliYamlParser) quotedValue(value string, parent int) Value {
	quote := value[0]
	out := string(quote)
	line := value[1:]
	for {
		if end := cliClosingQuote(line, quote, 0); end != -1 {
			rest := line[end+1:]
			if cliTrim(rest) != "" && !cliYamlCommentAfter.MatchString(rest) {
				panic(cliSyntax("Unexpected content after quoted YAML scalar"))
			}
			return cliScalar(out + line[:end+1])
		}
		if quote == '"' && cliEndsWithEscape(line) {
			out += line[:len(line)-1]
		} else {
			out += cliTrimFoldedEnd(line, quote) + cliFold(p.skipBlankLines())
		}
		next := p.line()
		p.i++
		if next == nil {
			panic(cliSyntax("Unterminated quoted YAML scalar"))
		}
		if cliTrim(next.raw) != "" && next.indent <= parent {
			p.fail(next, "Invalid quoted scalar indentation")
		}
		line = cliTrimStart(next.raw)
	}
}

var cliBlockIndicator = regexp.MustCompile(`[^1-9]`)

func (p *cliYamlParser) blockScalar(indicator string, parent int) Value {
	style := indicator[0]
	chomp := "clip"
	if strings.Contains(indicator, "-") {
		chomp = "strip"
	} else if strings.Contains(indicator, "+") {
		chomp = "keep"
	}
	start := p.i
	end := start
	content := -1
	if digits := cliBlockIndicator.ReplaceAllString(indicator, ""); digits != "" {
		explicit, _ := strconv.Atoi(digits)
		content = parent + explicit
	}
	for end < len(p.lines) {
		l := &p.lines[end]
		if cliTrim(l.raw) != "" && l.indent <= parent {
			break
		}
		if cliTrim(l.raw) != "" && (content < 0 || l.indent < content) {
			content = l.indent
		}
		end++
	}
	if content < 0 {
		content = parent + 1
	}
	var lines []string
	for i := start; i < end; i++ {
		l := &p.lines[i]
		switch {
		case cliTrim(l.raw) == "":
			lines = append(lines, "")
		case l.indent < content:
			p.fail(l, "Expected block scalar indentation of "+strconv.Itoa(content)+" spaces")
		default:
			lines = append(lines, l.raw[content:])
		}
	}
	p.i = end
	contentEnd := len(lines)
	for contentEnd > 0 && lines[contentEnd-1] == "" {
		contentEnd--
	}
	atEnd := 0
	if end == len(p.lines) {
		atEnd = 1
	}
	trailing := max(0, len(lines)-contentEnd-atEnd)
	if contentEnd > 0 && start+contentEnd < len(p.lines) {
		trailing++
	}
	lines = lines[:contentEnd]
	out := ""
	if style == '|' {
		out = strings.Join(lines, "\n")
	} else {
		blank, previous := 0, -1 // previous: whether the last line was more indented, -1 before any
		for _, line := range lines {
			if line == "" {
				blank++
				continue
			}
			more := strings.HasPrefix(line, " ")
			hard := more || previous == 1
			switch {
			case previous == -1:
				out += strings.Repeat("\n", blank)
			case blank == 0 && hard:
				out += "\n"
			case blank == 0:
				out += " "
			case hard:
				out += strings.Repeat("\n", blank+1)
			default:
				out += strings.Repeat("\n", blank)
			}
			out += line
			blank, previous = 0, 0
			if more {
				previous = 1
			}
		}
	}
	if chomp == "keep" {
		return out + strings.Repeat("\n", trailing)
	}
	if chomp == "clip" && contentEnd > 0 && trailing > 0 {
		return out + "\n"
	}
	return out
}

func (p *cliYamlParser) skipBlankLines() int {
	start := p.i
	for p.i < len(p.lines) && cliTrim(p.lines[p.i].raw) == "" {
		p.i++
	}
	return p.i - start
}

func (p *cliYamlParser) skipIgnored() {
	for p.i < len(p.lines) {
		l := &p.lines[p.i]
		text := cliTrimEnd(l.text)
		switch {
		case text == "" || strings.HasPrefix(text, "%"):
			p.i++
		case l.indent == 0 && text == "---":
			if p.started {
				p.fail(l, "Multiple YAML documents are not supported")
			}
			p.started = true
			p.i++
		case l.indent == 0 && text == "...":
			p.i++
			for p.i < len(p.lines) && cliTrimEnd(p.lines[p.i].text) == "" {
				p.i++
			}
			if p.i < len(p.lines) {
				p.fail(&p.lines[p.i], "Multiple YAML documents are not supported")
			}
		default:
			return
		}
	}
}
