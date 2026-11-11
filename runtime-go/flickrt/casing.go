package flickrt

import (
	"regexp"
	"strings"
)

// The `String` functions on lines, margins and the case of identifiers, on
// code units as JS runs them.

// lines splits at CR, LF and CRLF; a break at the end adds no line.
func lines(s string, keep bool) Value {
	units := utf16Units(s)
	out := []Value{}
	for i := 0; i < len(units); {
		start := i
		for i < len(units) && units[i] != '\r' && units[i] != '\n' {
			i++
		}
		end := i
		if i < len(units) {
			if units[i] == '\r' && i+1 < len(units) && units[i+1] == '\n' {
				i++
			}
			i++
			if keep {
				end = i
			}
		}
		out = append(out, fromUnits(units[start:end]))
	}
	return out
}

func Lines(s Value) Value               { return lines(s.(string), false) }
func LinesWithSeparators(s Value) Value { return lines(s.(string), true) }

// StripMarginWith drops, from each line, what comes before `margin` when
// only spaces and control characters do.
func StripMarginWith(margin, s Value) Value {
	m := utf16Units(margin.(string))
	var out []uint16
	for _, line := range List(LinesWithSeparators(s)) {
		units := utf16Units(line.(string))
		i := 0
		for i < len(units) && units[i] <= ' ' {
			i++
		}
		if i < len(units) && len(m) > 0 && units[i] == m[0] {
			units = units[i+1:]
		}
		out = append(out, units...)
	}
	return fromUnits(out)
}

func StripMargin(s Value) Value { return StripMarginWith("|", s) }

func unit(units []uint16, i int) string {
	if i >= len(units) {
		return ""
	}
	return fromUnits(units[i : i+1])
}

func snakeTo(s string, pascal bool) Value {
	units := utf16Units(s)
	if len(units) == 0 || len(units)%2 == 0 && strings.Trim(s, "_") == "" {
		return ""
	}
	out := unit(units, 0)
	if pascal {
		out = ToUpperCase(out).(string)
	}
	for i := 1; i < len(units); i++ {
		if units[i] == '_' {
			i++
			out += ToUpperCase(unit(units, i)).(string)
		} else {
			out += unit(units, i)
		}
	}
	return out
}

func SnakeToCamel(s Value) Value  { return snakeTo(s.(string), false) }
func SnakeToPascal(s Value) Value { return snakeTo(s.(string), true) }
func SnakeToKebab(s Value) Value  { return strings.ReplaceAll(s.(string), "_", "-") }
func KebabToSnake(s Value) Value  { return strings.ReplaceAll(s.(string), "-", "_") }

var upperLetter = regexp.MustCompile(`([A-Z])`)

func CamelToSnake(s Value) Value {
	return ToLowerCase(upperLetter.ReplaceAllString(s.(string), "_${1}"))
}

func PascalToSnake(s Value) Value {
	units := utf16Units(s.(string))
	if len(units) == 0 {
		return ""
	}
	return ToLowerCase(unit(units, 0) + upperLetter.ReplaceAllString(fromUnits(units[1:]), "_${1}"))
}

var (
	splitCase = []*regexp.Regexp{
		regexp.MustCompile(`([a-z0-9])([A-Z])`),
		regexp.MustCompile(`([A-Z])([A-Z][a-z])`),
		regexp.MustCompile(`(?i)([A-Z])([0-9])`),
		regexp.MustCompile(`(?i)([0-9])([A-Z])`),
	}
	stripCase = regexp.MustCompile(`(?i)[^A-Z0-9]+`)
)

// words is the words of an identifier, each transformed, joined by
// `delimiter`.
func words(s string, splits []*regexp.Regexp, delimiter string, transform func(w string, i int) string) Value {
	for _, re := range splits {
		s = re.ReplaceAllString(s, "${1}\x00${2}")
	}
	s = strings.Trim(stripCase.ReplaceAllString(s, "\x00"), "\x00")
	if s == "" {
		return ""
	}
	parts := strings.Split(s, "\x00")
	for i, p := range parts {
		parts[i] = transform(p, i)
	}
	return strings.Join(parts, delimiter)
}

func lower(w string, _ int) string { return ToLowerCase(w).(string) }
func upper(w string, _ int) string { return ToUpperCase(w).(string) }

func pascalWord(w string, _ int) string {
	units := utf16Units(w)
	return ToUpperCase(unit(units, 0)).(string) + ToLowerCase(fromUnits(units[min(1, len(units)):])).(string)
}

func NoCase(s Value) Value       { return words(s.(string), splitCase, " ", lower) }
func KebabCase(s Value) Value    { return words(s.(string), splitCase, "-", lower) }
func SnakeCase(s Value) Value    { return words(s.(string), splitCase, "_", lower) }
func ConstantCase(s Value) Value { return words(s.(string), splitCase, "_", upper) }
func PascalCase(s Value) Value   { return words(s.(string), splitCase, "", pascalWord) }
func ConfigCase(s Value) Value   { return words(s.(string), splitCase[:2], "_", upper) }

func CamelCase(s Value) Value {
	return words(s.(string), splitCase, "", func(w string, i int) string {
		if i == 0 {
			return lower(w, i)
		}
		return pascalWord(w, i)
	})
}
