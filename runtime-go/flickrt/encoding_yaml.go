package flickrt

import (
	"strconv"
	"strings"
)

// parseYaml is Effect's encoding/Yaml `parse` (based on `yaml` 2.9.0): one
// document of block and flow collections, plain, quoted and block
// scalars, anchors, aliases and merge keys (`<<`). As Effect's, it reads
// no tags, compact nested sequences, multi-line quoted scalars in flow
// collections or complex keys.
func parseYaml(input string) (v Value, err string) {
	defer func() {
		if r := recover(); r != nil {
			e, ok := r.(encodingError)
			if !ok {
				panic(r)
			}
			v, err = nil, string(e)
		}
	}()
	source := strings.ReplaceAll(strings.TrimPrefix(input, "\uFEFF"), "\r\n", "\n")
	source = strings.ReplaceAll(source, "\r", "\n")
	raws := strings.Split(source, "\n")
	lines := make([]yamlLine, len(raws))
	for i, raw := range raws {
		indent := len(raw) - len(strings.TrimLeft(raw, " "))
		// rejected where the line is read as structure; block scalar content may hold tabs
		tabIndent := indent < len(raw) && raw[indent] == '\t'
		lines[i] = yamlLine{raw, stripComment(raw[indent:], 0), indent, i + 1, tabIndent}
	}
	p := &yamlParser{lines: lines, anchors: map[string]Value{}}
	return p.parse(), ""
}

type yamlLine struct {
	raw, text      string // text: without indentation and comment
	indent, number int
	tabIndent      bool
}

type yamlParser struct {
	lines           []yamlLine
	index           int
	documentStarted bool
	anchors         map[string]Value
}

// at is the line at `i`, nil past the last.
func (p *yamlParser) at(i int) *yamlLine {
	if i < len(p.lines) {
		return &p.lines[i]
	}
	return nil
}

func isBlank(s string) bool { return strings.TrimFunc(s, isJSSpace) == "" }

func (p *yamlParser) parse() Value {
	p.skipIgnored()
	if p.index >= len(p.lines) {
		return JsonNull
	}
	p.documentStarted = true
	indent := p.lines[p.index].indent
	v := p.parseNode(indent, indent-1)
	p.skipIgnored()
	if line := p.at(p.index); line != nil {
		p.fail(line, "Unexpected content")
	}
	return v
}

func (p *yamlParser) parseNode(indent, parentIndent int) Value {
	p.skipIgnored()
	line := p.at(p.index)
	if line == nil {
		return JsonNull
	}
	if line.indent != indent {
		p.fail(line, "Expected indentation of "+strconv.Itoa(indent)+" spaces")
	}
	if isSequenceEntry(line.text) {
		return p.parseSequence(indent)
	}
	if findIndicator(line.text, ':', 0) != -1 {
		return p.parseMapping(indent, newYamlMapping())
	}
	p.index++
	return p.parseNodeValue(line.text, parentIndent, false)
}

func (p *yamlParser) parseMapping(indent int, mapping *yamlMapping) *Record {
	for {
		p.skipIgnored()
		line := p.at(p.index)
		if line == nil || line.indent < indent {
			return mapping.out
		}
		if line.indent > indent {
			p.fail(line, "Unexpected indentation of "+strconv.Itoa(line.indent)+" spaces")
		}
		separator := findIndicator(line.text, ':', 0)
		if separator == -1 {
			return mapping.out
		}
		p.index++
		p.parseMappingEntry(mapping, line.text, separator, indent, line)
	}
}

func (p *yamlParser) parseMappingEntry(mapping *yamlMapping, text string, separator, indent int, line *yamlLine) {
	rawKey := strings.TrimFunc(text[:separator], isJSSpace)
	value := p.parseNodeValue(strings.TrimLeftFunc(text[separator+1:], isJSSpace), indent, true)
	fail := func(message string) { p.fail(line, message) }
	if rawKey == "<<" {
		mapping.merge(value, fail)
	} else {
		mapping.set(parseYamlKey(rawKey), value, fail)
	}
}

func (p *yamlParser) parseSequence(indent int) []Value {
	out := []Value{}
	for {
		p.skipIgnored()
		line := p.at(p.index)
		if line == nil || line.indent < indent {
			return out
		}
		if line.indent > indent {
			p.fail(line, "Unexpected indentation of "+strconv.Itoa(line.indent)+" spaces")
		}
		if !isSequenceEntry(line.text) {
			return out
		}
		p.index++
		item := strings.TrimLeftFunc(line.text[1:], isJSSpace)
		if isSequenceEntry(item) {
			p.fail(line, "Compact nested block sequences are not supported")
		}
		if separator := findIndicator(item, ':', 0); separator == -1 {
			out = append(out, p.parseNodeValue(item, indent, false))
		} else {
			// the mapping is indented to its first key, which may sit more than one space after the dash
			dash := line.text[:len(line.text)-len(item)]
			if strings.Contains(dash[1:], "\t") {
				p.fail(line, "Tabs cannot be used for YAML indentation")
			}
			mappingIndent := indent + utf16Len(dash)
			mapping := newYamlMapping()
			p.parseMappingEntry(mapping, item, separator, mappingIndent, line)
			out = append(out, p.parseMapping(mappingIndent, mapping))
		}
	}
}

// yamlAnchor is /^&([^\s,[\]{}]+)(?:\s+(.*))?$/ of a value: its anchor's
// name and the rest.
func yamlAnchor(raw string) (name, rest string, ok bool) {
	if !strings.HasPrefix(raw, "&") {
		return "", "", false
	}
	end := 1
	for end < len(raw) {
		r, n := decode(raw[end:])
		if isJSSpace(r) || strings.ContainsRune(",[]{}", r) {
			break
		}
		end += n
	}
	if end == 1 {
		return "", "", false
	}
	if end == len(raw) {
		return raw[1:end], "", true
	}
	if !isSpaceAt(raw, end) {
		return "", "", false
	}
	rest = strings.TrimLeftFunc(raw[end:], isJSSpace)
	// `.` does not match a line terminator
	if strings.ContainsAny(rest, "\u2028\u2029") {
		return "", "", false
	}
	return raw[1:end], rest, true
}

func (p *yamlParser) parseNodeValue(raw string, parentIndent int, allowIndentless bool) Value {
	name, rest, ok := yamlAnchor(raw)
	if !ok {
		return p.parseValue(raw, parentIndent, allowIndentless)
	}
	v := p.parseValue(rest, parentIndent, allowIndentless)
	p.anchors[name] = v
	return v
}

func isBlockScalarHeader(text string) bool {
	if text == "" || text[0] != '|' && text[0] != '>' {
		return false
	}
	h := text[1:]
	digit := func(i int) bool { return i < len(h) && h[i] >= '1' && h[i] <= '9' }
	chomp := func(i int) bool { return i < len(h) && (h[i] == '+' || h[i] == '-') }
	// [1-9]?[+-]? or [+-]?[1-9]?
	switch len(h) {
	case 0:
		return true
	case 1:
		return digit(0) || chomp(0)
	case 2:
		return digit(0) && chomp(1) || chomp(0) && digit(1)
	}
	return false
}

func (p *yamlParser) parseValue(value string, parentIndent int, allowIndentless bool) Value {
	text := strings.TrimRightFunc(value, isJSSpace)
	switch {
	case text == "":
		p.skipIgnored()
		next := p.at(p.index)
		switch {
		case next == nil:
			return JsonNull
		case next.indent > parentIndent:
			return p.parseNode(next.indent, parentIndent)
		case allowIndentless && next.indent == parentIndent && isSequenceEntry(next.text):
			return p.parseSequence(parentIndent)
		}
		return JsonNull
	case isBlockScalarHeader(text):
		return p.parseBlockScalar(text, parentIndent)
	case value[0] == '"' || value[0] == '\'':
		// escaped trailing white space in quoted values is content
		return p.parseQuotedValue(value, parentIndent)
	case text[0] == '*':
		name := strings.TrimFunc(text[1:], isJSSpace)
		v, ok := p.anchors[name]
		if !ok {
			panic(encodingError("SyntaxError: Unknown YAML alias '*" + name + "'"))
		}
		return v
	case text[0] == '[' || text[0] == '{':
		return p.parseFlowValue(text, parentIndent)
	}
	return p.parsePlainValue(text, parentIndent)
}

// parseFlowValue is a flow collection, which goes on over the next lines
// while it is open.
func (p *yamlParser) parseFlowValue(text string, parentIndent int) Value {
	nextLine := func(depth int) (string, bool) {
		next := p.at(p.index)
		if next == nil || next.indent == 0 && isDocumentMarker(next.text) {
			return "", false
		}
		// continuation lines are more indented than the parent; a closing bracket may sit at its indentation
		content := !isBlank(next.text)
		if content && next.indent < parentIndent {
			return "", false
		}
		if content && next.indent == parentIndent && next.text[0] != ']' && next.text[0] != '}' {
			return "", false
		}
		p.index++
		// strip comments again knowing the open collections, so quoted `#` after a comma is kept
		return stripComment(next.raw[next.indent:], depth), true
	}
	return (&yamlFlow{in: text, anchors: p.anchors, nextLine: nextLine}).parse()
}

// plainComment is the index of /(?:^|[ \t])#/ in `s`, -1 for none.
func plainComment(s string) int {
	if strings.HasPrefix(s, "#") {
		return 0
	}
	for i := 0; i+1 < len(s); i++ {
		if (s[i] == ' ' || s[i] == '\t') && s[i+1] == '#' {
			return i
		}
	}
	return -1
}

func (p *yamlParser) parsePlainValue(value string, parentIndent int) Value {
	first := &p.lines[p.index-1]
	if hasValueColon(value) {
		p.fail(first, "Invalid colon in plain scalar")
	}
	out := value
	terminated := findIndicator(first.raw, '#', 0) != -1
	for !terminated {
		blankLines := p.skipBlankLines()
		next := p.at(p.index)
		if next == nil || next.indent <= parentIndent {
			break
		}
		p.rejectTabIndent(next)
		// quotes in plain continuations are literal
		content := next.raw[next.indent:]
		comment := plainComment(content)
		text := content
		if comment != -1 {
			text = content[:comment]
		}
		text = strings.TrimRightFunc(text, isJSSpace)
		if text == "" || next.indent == 0 && isDocumentMarker(text) {
			break
		}
		if hasValueColon(text) {
			p.fail(next, "Invalid colon in plain scalar")
		}
		p.rejectTabIndentedBlankLines(blankLines, parentIndent)
		out += foldLineBreak(blankLines) + text
		p.index++
		terminated = comment != -1
	}
	return parseYamlScalar(out)
}

// isDocumentMarker is /^(?:---|\.\.\.)(?:\s|$)/.
func isDocumentMarker(text string) bool {
	return (strings.HasPrefix(text, "---") || strings.HasPrefix(text, "...")) && (len(text) == 3 || isSpaceAt(text, 3))
}

func (p *yamlParser) parseQuotedValue(value string, parentIndent int) Value {
	quote := value[0]
	out := value[:1]
	line := value[1:]
	for {
		if end := closingQuote(line, quote, 0); end != -1 {
			rest := line[end+1:]
			if !isBlank(rest) && !(strings.TrimLeft(rest, " \t") != rest && strings.HasPrefix(strings.TrimLeft(rest, " \t"), "#")) {
				panic(encodingError("SyntaxError: Unexpected content after quoted YAML scalar"))
			}
			return parseYamlScalar(out + line[:end+1])
		}
		if quote == '"' && endsWithEscape(line) {
			out += line[:len(line)-1]
		} else {
			blankLines := p.skipBlankLines()
			p.rejectTabIndentedBlankLines(blankLines, parentIndent)
			out += trimFoldedEnd(line, quote) + foldLineBreak(blankLines)
		}
		next := p.at(p.index)
		p.index++
		if next == nil {
			panic(encodingError("SyntaxError: Unterminated quoted YAML scalar"))
		}
		p.rejectTabIndent(next)
		if !isBlank(next.raw) && next.indent <= parentIndent {
			p.fail(next, "Invalid quoted scalar indentation")
		}
		line = strings.TrimLeftFunc(next.raw, isJSSpace)
	}
}

func (p *yamlParser) parseBlockScalar(indicator string, parentIndent int) string {
	style := indicator[0]
	keep, strip := strings.Contains(indicator, "+"), strings.Contains(indicator, "-")
	start := p.index
	end := start
	contentIndent := -1 // unset; an indentation is at least 0
	if digits := strings.Trim(indicator[1:], "+-"); digits != "" {
		contentIndent = parentIndent + int(digits[0]-'0')
	}
	hasContent := false
	for ; end < len(p.lines); end++ {
		line := &p.lines[end]
		// only spaces make a line empty; a tab after the indentation is content
		if strings.Trim(line.raw, " ") == "" {
			continue
		}
		if contentIndent == -1 && line.indent > parentIndent {
			// the first non-empty line sets the indentation, which no leading empty line may exceed
			for i := start; i < end; i++ {
				if p.lines[i].indent > line.indent {
					p.fail(&p.lines[i], "Leading empty lines cannot be more indented than the block scalar content")
				}
			}
			contentIndent = line.indent
		}
		if contentIndent != -1 && line.indent >= contentIndent {
			hasContent = true
			continue
		}
		p.rejectTabIndent(line)
		// a less indented line or comment ends the scalar
		if line.indent <= parentIndent || line.text == "" {
			break
		}
		p.fail(line, "Expected block scalar indentation of "+strconv.Itoa(contentIndent)+" spaces")
	}

	// white space past the indentation is content only once the scalar has a content line
	content := make([]string, end-start)
	if hasContent {
		for i := range content {
			raw := p.lines[start+i].raw
			content[i] = raw[min(contentIndent, len(raw)):]
		}
	}
	p.index = end

	contentEnd := len(content)
	for contentEnd > 0 && content[contentEnd-1] == "" {
		contentEnd--
	}
	trailingBreaks := len(content) - contentEnd
	if end == len(p.lines) {
		trailingBreaks--
	}
	trailingBreaks = max(0, trailingBreaks)
	if contentEnd > 0 && start+contentEnd < len(p.lines) {
		trailingBreaks++
	}
	content = content[:contentEnd]

	var out strings.Builder
	if style == '|' {
		out.WriteString(strings.Join(content, "\n"))
	} else {
		blankLines, started, previousMoreIndented := 0, false, false
		for _, line := range content {
			if line == "" {
				blankLines++
				continue
			}
			moreIndented := line[0] == ' ' || line[0] == '\t'
			hardBreak := moreIndented || started && previousMoreIndented
			switch {
			case !started:
				out.WriteString(strings.Repeat("\n", blankLines))
			case blankLines == 0 && hardBreak:
				out.WriteString("\n")
			case blankLines == 0:
				out.WriteString(" ")
			case hardBreak:
				out.WriteString(strings.Repeat("\n", blankLines+1))
			default:
				out.WriteString(strings.Repeat("\n", blankLines))
			}
			out.WriteString(line)
			blankLines, started, previousMoreIndented = 0, true, moreIndented
		}
	}
	switch {
	case keep && !strip:
		return out.String() + strings.Repeat("\n", trailingBreaks)
	case !keep && !strip && contentEnd > 0 && trailingBreaks > 0:
		return out.String() + "\n"
	}
	return out.String()
}

func (p *yamlParser) skipBlankLines() int {
	start := p.index
	for p.index < len(p.lines) && isBlank(p.lines[p.index].raw) {
		p.index++
	}
	return p.index - start
}

func (p *yamlParser) skipIgnored() {
	for p.index < len(p.lines) {
		line := &p.lines[p.index]
		text := strings.TrimRightFunc(line.text, isJSSpace)
		switch {
		case text == "" || strings.HasPrefix(text, "%"):
			p.index++
		case line.indent == 0 && text == "---":
			if p.documentStarted {
				p.fail(line, "Multiple YAML documents are not supported")
			}
			p.documentStarted = true
			p.index++
		case line.indent == 0 && text == "...":
			p.index++
			for p.index < len(p.lines) && strings.TrimRightFunc(p.lines[p.index].text, isJSSpace) == "" {
				p.index++
			}
			if line := p.at(p.index); line != nil {
				p.fail(line, "Multiple YAML documents are not supported")
			}
		default:
			p.rejectTabIndent(line)
			return
		}
	}
}

func (p *yamlParser) rejectTabIndent(line *yamlLine) {
	if line.tabIndent {
		p.fail(line, "Tabs cannot be used for YAML indentation")
	}
}

func (p *yamlParser) rejectTabIndentedBlankLines(count, parentIndent int) {
	for i := p.index - count; i < p.index; i++ {
		if line := &p.lines[i]; line.indent <= parentIndent {
			p.rejectTabIndent(line)
		}
	}
}

func (p *yamlParser) fail(line *yamlLine, message string) {
	panic(encodingError("SyntaxError: " + message + " at line " + strconv.Itoa(line.number)))
}
