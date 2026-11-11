package flickrt

import (
	"regexp"
	"strconv"
	"strings"
)

// Regular expressions run on Go's RE2 engine: a pattern written for JS
// within what RE2 reads behaves the same, positions count UTF-16 code
// units, and replacements read JS's `$` patterns.
//
// shortcut: JS-only syntax (lookaround, backreferences) is a RegExpError, and `\s` and `.` read ASCII line breaks; translate them when a program needs them

// RegExp is a compiled pattern and whether it is global.
type RegExp struct {
	re     *regexp.Regexp
	global bool
}

var regexpTag = Tag("RegExp.RegExpError.RegExpError")

func regexpError(message string) Value { return Err(NewData(regexpTag, message)) }

// RegExpMake compiles `pattern` with JS flags: `i`, `m` and `s` as RE2's,
// `g` global; the sticky `y` is refused.
func RegExpMake(pattern, flags Value) Value {
	f := flags.(string)
	if strings.Contains(f, "y") {
		return regexpError("the sticky flag `y` is not supported")
	}
	inline := ""
	for _, c := range "ims" {
		if strings.ContainsRune(f, c) {
			inline += string(c)
		}
	}
	source := pattern.(string)
	if inline != "" {
		source = "(?" + inline + ")" + source
	}
	re, err := regexp.Compile(source)
	if err != nil {
		return regexpError("Invalid regular expression: /" + pattern.(string) + "/" + f + ": " + err.Error())
	}
	return Ok(&RegExp{re, strings.Contains(f, "g")})
}

var escaped = regexp.MustCompile(`[/\\^$*+?.()|[\]{}]`)

// RegExpEscape escapes what JS reads as syntax in a pattern.
func RegExpEscape(s Value) Value { return escaped.ReplaceAllString(s.(string), `\$0`) }

func match(s string, loc []int) Value {
	captures := make([]Value, len(loc)/2-1)
	for i := range captures {
		if a := loc[2*i+2]; a < 0 {
			captures[i] = None
		} else {
			captures[i] = Some(s[a:loc[2*i+3]])
		}
	}
	return Rec("matched", s[loc[0]:loc[1]], "index", indexUnits(s, loc[0]), "captures", captures)
}

func RegExpMatch(re, s Value) Value {
	text := s.(string)
	if loc := re.(*RegExp).re.FindStringSubmatchIndex(text); loc != nil {
		return Some(match(text, loc))
	}
	return None
}

func RegExpMatchAll(re, s Value) Value {
	text := s.(string)
	out := []Value{}
	for _, loc := range re.(*RegExp).re.FindAllStringSubmatchIndex(text, -1) {
		out = append(out, match(text, loc))
	}
	return out
}

// RegExpSearch is the code unit position of the first match, if any.
func RegExpSearch(re, s Value) Value {
	text := s.(string)
	if loc := re.(*RegExp).re.FindStringIndex(text); loc != nil {
		return Some(indexUnits(text, loc[0]))
	}
	return None
}

// expand is JS's replacement of a match: `$$`, `$&`, “ $` “, `$'`, and
// `$n` or `$nn` for a group that exists.
func expand(replacement, s string, loc []int) string {
	var b strings.Builder
	groups := len(loc)/2 - 1
	group := func(n int) string {
		if loc[2*n] < 0 {
			return ""
		}
		return s[loc[2*n]:loc[2*n+1]]
	}
	for i := 0; i < len(replacement); i++ {
		c := replacement[i]
		if c != '$' || i+1 == len(replacement) {
			b.WriteByte(c)
			continue
		}
		switch next := replacement[i+1]; {
		case next == '$':
			b.WriteByte('$')
			i++
		case next == '&':
			b.WriteString(group(0))
			i++
		case next == '`':
			b.WriteString(s[:loc[0]])
			i++
		case next == '\'':
			b.WriteString(s[loc[1]:])
			i++
		case next >= '0' && next <= '9':
			n, width := int(next-'0'), 1
			if i+2 < len(replacement) && replacement[i+2] >= '0' && replacement[i+2] <= '9' {
				if two, _ := strconv.Atoi(replacement[i+1 : i+3]); two >= 1 && two <= groups {
					n, width = two, 2
				}
			}
			if n >= 1 && n <= groups {
				b.WriteString(group(n))
				i += width
			} else {
				b.WriteByte('$')
			}
		default:
			b.WriteByte('$')
		}
	}
	return b.String()
}

func replace(re *RegExp, replacement, s string, all bool) string {
	var b strings.Builder
	last := 0
	locs := re.re.FindAllStringSubmatchIndex(s, -1)
	if !all && len(locs) > 1 {
		locs = locs[:1]
	}
	for _, loc := range locs {
		b.WriteString(s[last:loc[0]])
		b.WriteString(expand(replacement, s, loc))
		last = loc[1]
	}
	b.WriteString(s[last:])
	return b.String()
}

// RegExpReplace replaces the first match, or every one of a global
// pattern, as JS `s.replace` does.
func RegExpReplace(re, replacement, s Value) Value {
	r := re.(*RegExp)
	return replace(r, replacement.(string), s.(string), r.global)
}

func RegExpReplaceAll(re, replacement, s Value) Value {
	return replace(re.(*RegExp), replacement.(string), s.(string), true)
}
