package flickrt

import (
	"regexp"
	"strings"
	"unicode/utf8"
)

// shortcut: route expressions use RE2 like RegExp, add JS regexp support when a route requires it

type routePattern struct {
	expr        *regexp.Regexp
	names       []string
	prefix      string
	suffix      string
	literals    []string
	constrained bool
}

type routeParam struct{ name, value string }

// patternSegments splits paths only outside a parameter's regexp.
func patternSegments(path string) []string {
	var out []string
	start := 0
	for i := 0; i < len(path); i++ {
		switch path[i] {
		case '/':
			out = append(out, path[start:i])
			start = i + 1
		case ':':
			if i+1 < len(path) && path[i+1] == ':' {
				i++
				continue
			}
			for j := i + 1; j < len(path) && !strings.ContainsRune("/.-", rune(path[j])); j++ {
				if path[j] == '(' {
					i = routeRegexEnd(path, j)
					break
				}
			}
		}
	}
	return append(out, path[start:])
}

func routeRegexEnd(pattern string, start int) int {
	depth, class := 1, false
	for i := start + 1; i < len(pattern); i++ {
		switch pattern[i] {
		case '\\':
			i++
		case '[':
			class = true
		case ']':
			class = false
		case '(':
			if !class {
				depth++
			}
		case ')':
			if !class {
				depth--
				if depth == 0 {
					return i
				}
			}
		}
	}
	panic(Defect{"Invalid route regexp in " + pattern})
}

func parameterStart(pattern string, start int) int {
	for i := start; i < len(pattern); i++ {
		if pattern[i] == ':' {
			if i+1 < len(pattern) && pattern[i+1] == ':' {
				i++
			} else {
				return i
			}
		}
	}
	return len(pattern)
}

func routeSegment(pattern string) segment {
	start := parameterStart(pattern, 0)
	if start == len(pattern) {
		return segment{kind: segStatic, text: strings.ReplaceAll(pattern, "::", ":")}
	}
	var expression strings.Builder
	expression.WriteByte('^')
	p := &routePattern{prefix: strings.ReplaceAll(pattern[:start], "::", ":")}
	p.literals = append(p.literals, p.prefix)
	plain, safe, unsupported := true, true, false
	for start < len(pattern) {
		end := start + 1
		for end < len(pattern) && !strings.ContainsRune("(.-", rune(pattern[end])) {
			end++
		}
		name := pattern[start+1 : end]
		p.names = append(p.names, name)
		if end == len(pattern) && start == 0 && len(p.names) == 1 {
			return segment{kind: segParam, text: name}
		}
		if end < len(pattern) && pattern[end] == '(' {
			plain, safe = false, true
			closing := routeRegexEnd(pattern, end)
			inner := strings.TrimSuffix(strings.TrimPrefix(pattern[end+1:closing], "^"), "$")
			expression.WriteByte('(')
			expression.WriteString(strings.ReplaceAll(inner, `\/`, "/"))
			expression.WriteByte(')')
			end = closing + 1
		} else {
			if safe {
				expression.WriteString("(.*?)")
			} else if utf8.RuneCountInString(p.suffix) == 1 {
				delimiter := regexp.QuoteMeta(p.suffix)
				expression.WriteString("(" + delimiter + "|[^" + delimiter + "]*)")
			} else {
				unsupported = true
			}
			safe = false
		}
		start = parameterStart(pattern, end)
		p.suffix = strings.ReplaceAll(pattern[end:start], "::", ":")
		p.literals = append(p.literals, p.suffix)
		expression.WriteString(regexp.QuoteMeta(p.suffix))
	}
	if !plain {
		if unsupported {
			panic(Defect{"Unsupported route regexp with a multi-character parameter separator in " + pattern})
		}
		expression.WriteByte('$')
		var err error
		p.expr, err = regexp.Compile(expression.String())
		if err != nil {
			panic(Defect{"Invalid or unsupported route regexp in " + pattern + ": " + err.Error()})
		}
	}
	p.constrained = !plain || len(p.names) > 1 || p.suffix != ""
	return segment{kind: segConstrained, pattern: p}
}
func (p *routePattern) sameMatcher(other *routePattern) bool {
	if p.expr != nil || other.expr != nil {
		return p.expr != nil && other.expr != nil && p.expr.String() == other.expr.String()
	}
	if len(p.literals) != len(other.literals) {
		return false
	}
	for i := 1; i < len(p.literals); i++ {
		if p.literals[i] != other.literals[i] {
			return false
		}
	}
	return true
}

func (p *routePattern) match(c routerConfig, value string, params []routeParam) ([]routeParam, bool) {
	if len(value) < len(p.prefix) || !c.same(p.prefix, value[:len(p.prefix)]) {
		return nil, false
	}
	value = value[len(p.prefix):]
	if p.expr == nil {
		return p.matchPlain(c, value, 0, 0, params)
	}
	groups := p.expr.FindStringSubmatchIndex(value)
	if groups == nil {
		return nil, false
	}
	for i := 2; i < len(groups); i += 2 {
		captured := ""
		if groups[i] >= 0 {
			captured = value[groups[i]:groups[i+1]]
		}
		if Length(captured).(int32) > c.maxParam {
			return nil, false
		}
		if name := i/2 - 1; name < len(p.names) {
			params = append(params, routeParam{p.names[name], captured})
		}
	}
	return params, true
}

// After the first plain parameter, FindMyWay excludes the preceding
// separator from a capture unless that capture is the separator itself.
func (p *routePattern) plainCapture(index int, value string) bool {
	if !p.constrained || index > 0 && value == p.literals[index] {
		return true
	}
	return !strings.ContainsAny(value, "\n\r\u2028\u2029") && (index == 0 || !strings.Contains(value, p.literals[index]))
}

func (p *routePattern) matchPlain(c routerConfig, value string, index, start int, params []routeParam) ([]routeParam, bool) {
	separator := p.literals[index+1]
	if index == len(p.names)-1 {
		if !strings.HasSuffix(value[start:], separator) {
			return nil, false
		}
		captured := value[start : len(value)-len(separator)]
		if Length(captured).(int32) > c.maxParam || !p.plainCapture(index, captured) {
			return nil, false
		}
		return append(params, routeParam{p.names[index], captured}), true
	}
	for end := start; end <= len(value); end++ {
		at := strings.Index(value[end:], separator)
		if at < 0 {
			break
		}
		end += at
		captured := value[start:end]
		if Length(captured).(int32) > c.maxParam {
			break
		}
		if p.plainCapture(index, captured) {
			next := append(params, routeParam{p.names[index], captured})
			if out, ok := p.matchPlain(c, value, index+1, end+len(separator), next); ok {
				return out, true
			}
		}
	}
	return nil, false
}
