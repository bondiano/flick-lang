//go:build flickcli

package flickrt

import (
	"regexp"
	"strings"
)

// Effect's `Primitive.FileParse`: a file read as INI, JSON, TOML or YAML by
// Effect's own parsers (`encoding/Ini.js`, `Toml.js`, `Yaml.js`), the
// value being its JSON text, as `run.ts` hands it to `Cli.Value`. The
// parsers build JSON values as `Json` has them, objects as records in the
// key order of a JS object.

// cliJSArray is a JS array a parser still pushes to.
type cliJSArray struct{ items []Value }

// cliFinish is the JSON value of a parsed tree, its arrays done.
func cliFinish(v Value) Value {
	switch x := v.(type) {
	case *cliJSArray:
		return cliFinish(x.items)
	case []Value:
		out := make([]Value, len(x))
		for i, e := range x {
			out[i] = cliFinish(e)
		}
		return out
	case *Record:
		out := newRecord(len(x.keys))
		for i, k := range x.keys {
			out.put(k, cliFinish(x.vals[i]))
		}
		return out
	}
	return v
}

// cliSyntaxError is a JS error a parser throws, as `String(error)` shows
// it.
type cliSyntaxError struct{ message string }

func cliSyntax(message string) cliSyntaxError { return cliSyntaxError{"SyntaxError: " + message} }

// cliParseText runs a parser, which panics with a cliSyntaxError.
func cliParseText(parse func(string) Value, content string) (out Value, problem string) {
	defer func() {
		if r := recover(); r != nil {
			e, ok := r.(cliSyntaxError)
			if !ok {
				panic(r)
			}
			problem = e.message
		}
	}()
	return parse(content), ""
}

// cliFileParse is the JSON text of the file, read in the format or by its
// extension.
func cliFileParse(path, format string) (Value, string) {
	if format == "" {
		format = path[strings.LastIndexByte(path, '.')+1:]
	}
	parsers := map[string]func(string) Value{
		"ini": cliIni, "toml": cliToml, "yml": cliYaml, "yaml": cliYaml,
		"json": func(text string) Value {
			v, reason := parseJSON(text)
			if reason != "" {
				panic(cliSyntax(reason))
			}
			return v
		},
	}
	parse, ok := parsers[format]
	if !ok {
		return nil, "Unsupported file format: " + format
	}
	content, problem := cliFileText(path)
	if problem != "" {
		return nil, problem
	}
	v, problem := cliParseText(parse, content.(string))
	if problem != "" {
		return nil, "Failed to parse '." + format + "' file content: " + problem
	}
	return JsonStringify(cliFinish(v)), ""
}

func cliIniSections(value string) []string {
	var out []string
	start := 0
	for i := 0; i < len(value); i++ {
		if value[i] == '.' && (i == 0 || value[i-1] != '\\') {
			out = append(out, value[start:i])
			start = i + 1
		}
	}
	return append(out, value[start:])
}

// cliIniUnquote is a quoted value as JSON reads it, or the text up to a
// comment.
func cliIniUnquote(input string) Value {
	value := strings.TrimFunc(input, isJSSpace)
	if len(value) >= 2 && (value[0] == '"' && value[len(value)-1] == '"' || value[0] == '\'' && value[len(value)-1] == '\'') {
		if value[0] == '\'' {
			value = value[1 : len(value)-1]
		}
		if v, reason := parseJSON(value); reason == "" {
			return v
		}
		return value
	}
	var out strings.Builder
	escaped := false
	for _, c := range value {
		switch {
		case escaped:
			if c != ';' && c != '#' && c != '\\' {
				out.WriteByte('\\')
			}
			out.WriteRune(c)
			escaped = false
		case c == ';' || c == '#':
			return strings.TrimFunc(out.String(), isJSSpace)
		case c == '\\':
			escaped = true
		default:
			out.WriteRune(c)
		}
	}
	return strings.TrimFunc(out.String(), isJSSpace)
}

// cliJSString is JS's `String(v)` of a JSON value.
func cliJSString(v Value) string {
	switch x := v.(type) {
	case string:
		return x
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			if e != JsonNull {
				parts[i] = cliJSString(e)
			}
		}
		return strings.Join(parts, ",")
	case *Record:
		return "[object Object]"
	case *jsonNull:
		return "null"
	}
	return Show(v).(string)
}

// cliJSSpace is the white space of a JS regular expression's `\s`.
const cliJSSpace = `[\t\n\v\f\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]`

var (
	cliIniLines    = regexp.MustCompile(`[\r\n]+`)
	cliIniSkip     = regexp.MustCompile(`^` + cliJSSpace + `*(?:[;#]|$)`)
	cliIniSection  = regexp.MustCompile(`^\[([^\]]*)\]` + cliJSSpace + `*$`)
	cliIniProperty = regexp.MustCompile(`^([^=]+)(=([^\n\r\x{2028}\x{2029}]*))?$`)
)

// cliIni is Effect's `Ini.parse`: sections, `key=value`, `key[]` arrays,
// and dotted sections nested.
//
// shortcut: a section named like an earlier array property gets a table of its own, where JS sets properties on the array; read it as JS does if such files turn up
func cliIni(input string) Value {
	output := &Record{}
	current := output
	scalar := "" // what a section names that is not a table, as JS shows it
	for _, line := range cliIniLines.Split(input, -1) {
		if line == "" || cliIniSkip.MatchString(line) {
			continue
		}
		if m := cliIniSection.FindStringSubmatch(line); m != nil {
			section := cliJSString(cliIniUnquote(m[1]))
			current, scalar = &Record{}, ""
			if section == "__proto__" {
				continue
			}
			if at := output.index(section); at >= 0 && output.vals[at] != JsonNull {
				switch x := output.vals[at].(type) {
				case *Record:
					current = x
				case string:
					scalar = "string '" + x + "'"
				case bool:
					scalar = "boolean '" + cliJSString(x) + "'"
				case float64:
					scalar = "number '" + cliJSString(x) + "'"
				}
				continue
			}
			output.put(section, current)
			continue
		}
		m := cliIniProperty.FindStringSubmatchIndex(line)
		if m == nil {
			continue
		}
		raw := cliIniUnquote(line[m[2]:m[3]])
		key := cliJSString(raw)
		rawKey, isText := raw.(string)
		array := isText && len(utf16Units(rawKey)) > 2 && strings.HasSuffix(rawKey, "[]")
		if array {
			key = rawKey[:len(rawKey)-2]
		}
		if key == "__proto__" {
			continue
		}
		if scalar != "" {
			panic(cliSyntaxError{"TypeError: Cannot create property '" + key + "' on " + scalar})
		}
		var value Value = true
		if m[4] >= 0 {
			value = cliIniUnquote(line[m[6]:m[7]])
			switch value {
			case "true":
				value = true
			case "false":
				value = false
			case "null":
				value = JsonNull
			}
		}
		at := current.index(key)
		if array {
			if at < 0 {
				current.put(key, &cliJSArray{})
			} else if _, ok := current.vals[at].(*cliJSArray); !ok {
				current.put(key, &cliJSArray{[]Value{current.vals[at]}})
			}
			at = current.index(key)
		}
		if at >= 0 {
			if a, ok := current.vals[at].(*cliJSArray); ok {
				a.items = append(a.items, value)
				continue
			}
		}
		current.put(key, value)
	}
	var remove []string
	for _, section := range append([]string(nil), output.keys...) {
		table, ok := output.vals[output.index(section)].(*Record)
		if !ok {
			continue
		}
		parts := cliIniSections(section)
		last := parts[len(parts)-1]
		key := strings.ReplaceAll(last, `\.`, ".")
		at := output
		for _, part := range parts[:len(parts)-1] {
			if part == "__proto__" {
				continue
			}
			j := at.index(part)
			if j < 0 {
				at.put(part, &Record{})
				j = at.index(part)
			} else if _, isTable := at.vals[j].(*Record); !isTable {
				at.vals[j] = &Record{}
			}
			at = at.vals[j].(*Record)
		}
		if at == output && key == last {
			continue
		}
		at.put(key, table)
		remove = append(remove, section)
	}
	for _, section := range remove {
		if i := output.index(section); i >= 0 {
			output.keys = append(output.keys[:i:i], output.keys[i+1:]...)
			output.vals = append(output.vals[:i:i], output.vals[i+1:]...)
		}
	}
	return output
}
