package flickrt

import (
	"os"
	"strconv"
	"strings"
)

// The primitives of Effect's CLI that read the file system: Path,
// FileText and FileParse. Paths resolve against the working directory as
// Node's `path.resolve` does.

func cliAbsolute(path string) string {
	if record(pathParse(path)).Get("root").(string) != "" {
		return path
	}
	return pathResolve(path)
}

// cliPathValue is Primitive.Path.
func cliPathValue(pathType string, mustExist bool, text string) (Value, string) {
	abs := cliAbsolute(text)
	info, err := os.Stat(abs)
	exists := err == nil
	if mustExist && !exists {
		return nil, "Path does not exist: " + abs
	}
	if exists && pathType == "file" && !info.Mode().IsRegular() {
		return nil, "Path is not a file: " + abs
	}
	if exists && pathType == "directory" && !info.IsDir() {
		return nil, "Path is not a directory: " + abs
	}
	return abs, ""
}

// cliFileText is Primitive.FileText.
func cliFileText(text string) (Value, string) {
	abs := cliAbsolute(text)
	info, err := os.Stat(abs)
	if err != nil {
		return nil, "File does not exist: " + abs
	}
	if !info.Mode().IsRegular() {
		return nil, "Path is not a file: " + abs
	}
	content, err := os.ReadFile(abs)
	if err != nil {
		return nil, "Failed to read file: " + err.Error()
	}
	return string(content), ""
}

// cliFileParse is Primitive.FileParse: the data of a file as a JSON value
// (`*Record` objects keep their key order).
func cliFileParse(format, text string) (Value, string) {
	if format == "" {
		format = text[strings.LastIndexByte(text, '.')+1:]
	}
	if format != "json" && format != "yaml" && format != "yml" && format != "ini" && format != "toml" {
		return nil, "Unsupported file format: " + format
	}
	content, issue := cliFileText(text)
	if issue != "" {
		return nil, issue
	}
	var v Value
	reason := ""
	switch format {
	case "json":
		v, reason = parseJSON(content.(string))
		if reason != "" {
			reason = "SyntaxError: " + reason
		}
	case "yaml", "yml":
		v, reason = cliYaml(content.(string))
	default:
		// shortcut: no INI or TOML reader yet; port Effect's encoding/Ini and encoding/Toml when a program reads one
		reason = "Error: " + format + " files are not supported on the go target yet"
	}
	if reason != "" {
		return nil, "Failed to parse '." + format + "' file content: " + reason
	}
	return v, ""
}

// cliYaml reads block mappings and sequences of plain or quoted scalars,
// with flow sequences and mappings of scalars.
// shortcut: a YAML subset (no anchors, tags, multi-line scalars or
// documents); port Effect's encoding/Yaml when a program needs more.
func cliYaml(text string) (Value, string) {
	lines := []cliYamlLine{}
	for _, raw := range strings.Split(strings.ReplaceAll(text, "\r\n", "\n"), "\n") {
		trimmed := strings.TrimSpace(cliYamlComment(raw))
		if trimmed == "" || trimmed == "---" {
			continue
		}
		lines = append(lines, cliYamlLine{len(raw) - len(strings.TrimLeft(raw, " ")), trimmed})
	}
	if len(lines) == 0 {
		return JsonNull, ""
	}
	p := &cliYamlParser{lines: lines}
	v := p.block(lines[0].indent)
	if p.err == "" && p.i < len(lines) {
		p.err = "unexpected line: " + lines[p.i].text
	}
	return v, p.err
}

type cliYamlLine struct {
	indent int
	text   string
}

type cliYamlParser struct {
	lines []cliYamlLine
	i     int
	err   string
}

func cliYamlComment(line string) string {
	quote := byte(0)
	for i := 0; i < len(line); i++ {
		switch c := line[i]; {
		case quote != 0 && c == quote:
			quote = 0
		case quote == 0 && (c == '"' || c == '\''):
			quote = c
		case quote == 0 && c == '#' && (i == 0 || line[i-1] == ' '):
			return line[:i]
		}
	}
	return line
}

func (p *cliYamlParser) block(indent int) Value {
	first := p.lines[p.i].text
	if first == "-" || strings.HasPrefix(first, "- ") {
		out := []Value{}
		for p.err == "" && p.i < len(p.lines) && p.lines[p.i].indent == indent && (p.lines[p.i].text == "-" || strings.HasPrefix(p.lines[p.i].text, "- ")) {
			item := strings.TrimSpace(p.lines[p.i].text[1:])
			if item == "" {
				p.i++
				out = append(out, p.nested(indent))
				continue
			}
			// an item that opens a mapping continues at the item's column
			p.lines[p.i] = cliYamlLine{indent + 2, item}
			if _, _, ok := cliYamlKey(item); ok {
				out = append(out, p.block(indent+2))
			} else {
				p.i++
				out = append(out, cliYamlScalar(item, &p.err))
			}
		}
		return out
	}
	if _, _, ok := cliYamlKey(first); !ok {
		p.i++
		return cliYamlScalar(first, &p.err)
	}
	out := &Record{}
	for p.err == "" && p.i < len(p.lines) && p.lines[p.i].indent == indent {
		key, rest, ok := cliYamlKey(p.lines[p.i].text)
		if !ok {
			p.err = "expected a key: " + p.lines[p.i].text
			break
		}
		p.i++
		if rest == "" {
			out.put(key, p.nested(indent))
		} else {
			out.put(key, cliYamlScalar(rest, &p.err))
		}
	}
	return out
}

// nested is the block under a line at `indent`, `null` when there is none.
func (p *cliYamlParser) nested(indent int) Value {
	if p.i < len(p.lines) && p.lines[p.i].indent > indent {
		return p.block(p.lines[p.i].indent)
	}
	if p.i < len(p.lines) && p.lines[p.i].indent == indent && strings.HasPrefix(p.lines[p.i].text, "- ") {
		return p.block(indent)
	}
	return JsonNull
}

// cliYamlKey splits `key: rest` outside quotes and brackets.
func cliYamlKey(text string) (string, string, bool) {
	if text[0] == '[' || text[0] == '{' {
		return "", "", false
	}
	quote := byte(0)
	for i := 0; i < len(text); i++ {
		switch c := text[i]; {
		case quote != 0 && c == quote:
			quote = 0
		case quote == 0 && (c == '"' || c == '\'') && i == 0:
			quote = c
		case quote == 0 && c == ':' && (i+1 == len(text) || text[i+1] == ' '):
			key := strings.TrimSpace(text[:i])
			if s, ok := cliYamlQuoted(key); ok {
				key = s
			}
			return key, strings.TrimSpace(text[i+1:]), true
		}
	}
	return "", "", false
}

func cliYamlQuoted(text string) (string, bool) {
	if len(text) >= 2 && text[0] == '"' && text[len(text)-1] == '"' {
		if s, err := strconv.Unquote(text); err == nil {
			return s, true
		}
	}
	if len(text) >= 2 && text[0] == '\'' && text[len(text)-1] == '\'' {
		return strings.ReplaceAll(text[1:len(text)-1], "''", "'"), true
	}
	return "", false
}

func cliYamlScalar(text string, err *string) Value {
	if text[0] == '[' || text[0] == '{' {
		v, rest := cliYamlFlow(strings.TrimSpace(text), err)
		if *err == "" && strings.TrimSpace(rest) != "" {
			*err = "unexpected text: " + rest
		}
		return v
	}
	if s, ok := cliYamlQuoted(text); ok {
		return s
	}
	switch text {
	case "null", "~", "Null", "NULL":
		return JsonNull
	case "true", "True", "TRUE":
		return true
	case "false", "False", "FALSE":
		return false
	}
	if n, e := strconv.ParseFloat(text, 64); e == nil && !strings.ContainsAny(text, "xXpP_") && text != "+" {
		return n
	}
	return text
}

// cliYamlFlow reads one flow collection or scalar; the text after it.
func cliYamlFlow(text string, err *string) (Value, string) {
	text = strings.TrimSpace(text)
	if text == "" {
		*err = "unexpected end of a flow collection"
		return nil, ""
	}
	if text[0] == '[' || text[0] == '{' {
		closing, list, rec := byte(']'), []Value{}, &Record{}
		if text[0] == '{' {
			closing = '}'
		}
		text = strings.TrimSpace(text[1:])
		for *err == "" {
			if text != "" && text[0] == closing {
				if closing == ']' {
					return list, text[1:]
				}
				return rec, text[1:]
			}
			var item Value
			if closing == '}' {
				key, rest, ok := cliYamlKey(text)
				if !ok {
					*err = "expected a key in a flow mapping"
					return nil, ""
				}
				item, text = cliYamlFlow(rest, err)
				rec.put(key, item)
			} else {
				item, text = cliYamlFlow(text, err)
				list = append(list, item)
			}
			text = strings.TrimSpace(text)
			if text != "" && text[0] == ',' {
				text = strings.TrimSpace(text[1:])
			} else if text == "" || text[0] != closing {
				*err = "unterminated flow collection"
			}
		}
		return nil, ""
	}
	end := strings.IndexAny(text, ",]}")
	if end < 0 {
		end = len(text)
	}
	return cliYamlScalar(strings.TrimSpace(text[:end]), err), text[end:]
}
