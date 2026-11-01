package flickrt

import (
	"strings"
)

// The parsers of Effect's `encoding` modules that `Cli.file_parse` reads
// files with, ported from effect 4.0.5: a file's data is a JSON value,
// objects `*Record`s still built in place with `put`. An error is the
// text of what the JS parser throws, `SyntaxError: …` or `TypeError: …`.

// parseIni is Effect's encoding/Ini `parse` (adapted from `ini` 7.0.0):
// dotted sections nest, repeated `key[]` make arrays, and `true`,
// `false` and `null` are decoded; other values stay strings.
//
// shortcut: a section that names an array (`a[]=1` then `[a]`) takes no keys, where JS sets an index-like key as an element; follow JS when a file does that
func parseIni(input string) (Value, string) {
	output := &Record{}
	var current Value = output
	lines := strings.FieldsFunc(input, func(r rune) bool { return r == '\r' || r == '\n' })
	for _, line := range lines {
		if rest := strings.TrimLeftFunc(line, isJSSpace); rest == "" || rest[0] == ';' || rest[0] == '#' {
			continue
		}
		if inner, ok := iniSection(line); ok {
			raw := iniUnquote(inner)
			section := stringOfJSON(raw)
			if raw == "__proto__" {
				current = &Record{}
				continue
			}
			current = lookup(output, section)
			if current == nil || current == JsonNull {
				current = &Record{}
				output.put(section, current)
			}
			continue
		}
		eq := strings.IndexByte(line, '=')
		if eq == 0 || eq > 0 && strings.ContainsAny(line[eq+1:], "\u2028\u2029") {
			continue // no match of /^([^=]+)(=(.*))?$/
		}
		keyText := line
		if eq > 0 {
			keyText = line[:eq]
		}
		rawKey := iniUnquote(keyText)
		key, err := iniKey(rawKey)
		if err != "" {
			return nil, err
		}
		s, isString := rawKey.(string)
		array := isString && utf16Len(s) > 2 && strings.HasSuffix(s, "[]")
		if array {
			key = s[:len(s)-2]
		}
		if key == "__proto__" {
			continue
		}
		var value Value = true
		if eq > 0 {
			value = iniUnquote(line[eq+1:])
			switch value {
			case "true":
				value = true
			case "false":
				value = false
			case "null":
				value = JsonNull
			}
		}
		table, ok := current.(*Record)
		if !ok {
			if _, isList := current.([]Value); isList {
				continue
			}
			return nil, "TypeError: Cannot create property '" + key + "' on " + jsTypeof(current) + " '" + stringOfJSON(current) + "'"
		}
		existing := lookup(table, key)
		if array {
			if existing == nil {
				existing = []Value{}
			} else if _, isList := existing.([]Value); !isList {
				existing = []Value{existing}
			}
		}
		if list, isList := existing.([]Value); isList {
			table.put(key, append(list, value))
		} else {
			table.put(key, value)
		}
	}

	var remove []string
	for _, section := range append([]string(nil), output.keys...) {
		table, ok := lookup(output, section).(*Record)
		if !ok {
			continue
		}
		parts := iniSplitSections(section)
		last := parts[len(parts)-1]
		parts = parts[:len(parts)-1]
		key := strings.ReplaceAll(last, `\.`, ".")
		at, intoList := output, false
		for _, part := range parts {
			if part == "__proto__" {
				continue
			}
			next, ok := lookup(at, part).(*Record)
			if _, isList := lookup(at, part).([]Value); isList {
				intoList = true // shortcut: see parseIni; JSON drops the property
				break
			}
			if !ok {
				next = &Record{}
				at.put(part, next)
			}
			at = next
		}
		if !intoList && at == output && key == last {
			continue
		}
		if !intoList {
			at.put(key, table)
		}
		remove = append(remove, section)
	}
	for _, section := range remove {
		output.remove(section)
	}
	return output, ""
}

// iniSection is the text between the brackets of a `[section]` line.
func iniSection(line string) (string, bool) {
	if line == "" || line[0] != '[' {
		return "", false
	}
	end := strings.IndexByte(line, ']')
	if end < 0 || strings.TrimLeftFunc(line[end+1:], isJSSpace) != "" {
		return "", false
	}
	return line[1:end], true
}

func iniSplitSections(value string) []string {
	var sections []string
	start := 0
	for i := 0; i < len(value); i++ {
		if value[i] == '.' && (i == 0 || value[i-1] != '\\') {
			sections = append(sections, value[start:i])
			start = i + 1
		}
	}
	return append(sections, value[start:])
}

// iniUnquote is a quoted value as JSON, or an unquoted one up to a
// comment, `\;`, `\#` and `\\` unescaped.
func iniUnquote(input string) Value {
	value := strings.TrimFunc(input, isJSSpace)
	if utf16Len(value) >= 2 && (value[0] == '"' && value[len(value)-1] == '"' || value[0] == '\'' && value[len(value)-1] == '\'') {
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
	for i := 0; i < len(value); {
		c, n := decode(value[i:])
		i += n
		switch {
		case escaped:
			if c != ';' && c != '#' && c != '\\' {
				out.WriteByte('\\')
			}
			out.WriteString(value[i-n : i])
			escaped = false
		case c == ';' || c == '#':
			return strings.TrimFunc(out.String(), isJSSpace)
		case c == '\\':
			escaped = true
		default:
			out.WriteString(value[i-n : i])
		}
	}
	return strings.TrimFunc(out.String(), isJSSpace)
}

// iniKey is the property key JS makes of an unquoted key: `String(v)`,
// or the TypeError reading `rawKey.length` or `rawKey.endsWith` throws.
func iniKey(v Value) (string, string) {
	switch x := v.(type) {
	case *jsonNull:
		return "", "TypeError: Cannot read properties of null (reading 'length')"
	case []Value:
		if len(x) > 2 {
			return "", "TypeError: rawKey.endsWith is not a function"
		}
	}
	return stringOfJSON(v), ""
}

// stringOfJSON is JS `String(v)` of a JSON value.
func stringOfJSON(v Value) string {
	switch x := v.(type) {
	case string:
		return x
	case float64:
		if x == 0 {
			return "0"
		}
		return ShowNumber(x)
	case bool:
		return Show(x).(string)
	case *jsonNull:
		return "null"
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			if e != JsonNull {
				parts[i] = stringOfJSON(e)
			}
		}
		return strings.Join(parts, ",")
	}
	return "[object Object]"
}

func jsTypeof(v Value) string {
	switch v.(type) {
	case string:
		return "string"
	case float64:
		return "number"
	case bool:
		return "boolean"
	}
	return "object"
}
