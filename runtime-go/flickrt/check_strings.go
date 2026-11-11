package flickrt

import (
	"regexp"
	"strconv"
	"strings"
)

// The checks of `Check` on strings: the same expected texts, JSON Schema
// keywords (exact or not) and arbitrary constraints as on the JS target.

// checkGroup is a group of checks: its checks run in order as if applied
// one by one, and the exporter states each.
type checkGroup []*check

// checksOf is the checks a `Check` value applies.
func checksOf(c Value) []*check {
	if g, ok := c.(checkGroup); ok {
		return g
	}
	return []*check{c.(*check)}
}

// CheckAll is `Check.all`: the group of the checks given.
func CheckAll(checks Value) Value {
	out := checkGroup{}
	for _, c := range List(checks) {
		out = append(out, checksOf(c)...)
	}
	return out
}

// CheckPredicate is `Check.predicate`: a filter with a message.
func CheckPredicate(f, message Value) Value {
	return &check{
		test:    func(fb *Fiber, v Value) bool { return Bool(Apply(fb, f, v)) },
		message: message.(string),
	}
}

func CheckLengthBetween(minimum, maximum Value) Value {
	low, high := cardinality(minimum), cardinality(maximum)
	c := isBetweenLength(low, high)
	c.arb = &arbConstraint{minLength: bound(float64(low)), maxLength: bound(float64(high))}
	return c
}

var CheckIsNonEmpty Value = CheckMinLength(int32(1))

// codePointsUpTo is the code points of `s`, counting no further than
// `limit`.
func codePointsUpTo(s string, limit int) int {
	count := 0
	for len(s) > 0 && count < limit {
		_, n := decode(s)
		s = s[n:]
		count++
	}
	return count
}

func codePoints(n int) string { return strconv.Itoa(n) + " code points" }

func CheckMinCodePoints(n Value) Value {
	minimum := cardinality(n)
	return &check{
		test:     func(_ *Fiber, v Value) bool { return codePointsUpTo(v.(string), minimum) >= minimum },
		expected: "a string with at least " + codePoints(minimum),
		keywords: exactly("minLength", float64(minimum)),
		arb:      &arbConstraint{minCodePoints: bound(float64(minimum))},
	}
}

func CheckMaxCodePoints(n Value) Value {
	maximum := cardinality(n)
	return &check{
		test:     func(_ *Fiber, v Value) bool { return codePointsUpTo(v.(string), maximum+1) <= maximum },
		expected: "a string with at most " + codePoints(maximum),
		keywords: exactly("maxLength", float64(maximum)),
		arb:      &arbConstraint{maxCodePoints: bound(float64(maximum))},
	}
}

func CheckCodePointsBetween(minimum, maximum Value) Value {
	low, high := cardinality(minimum), cardinality(maximum)
	expected := "a string with between " + strconv.Itoa(low) + " and " + codePoints(high)
	if low == high {
		expected = "a string with " + codePoints(low)
	}
	return &check{
		test: func(_ *Fiber, v Value) bool {
			n := codePointsUpTo(v.(string), high+1)
			return n >= low && n <= high
		},
		expected: expected,
		keywords: exactly("allOf", []Value{Rec("minLength", float64(low)), Rec("maxLength", float64(high))}),
		arb:      &arbConstraint{minCodePoints: bound(float64(low)), maxCodePoints: bound(float64(high))},
	}
}

// regExpSource is JS `new RegExp(pattern).source`: line terminators
// escaped. The patterns here have `/` escaped already.
func regExpSource(pattern string) string {
	return strings.NewReplacer("\n", `\n`, "\r", `\r`, " ", ` `, " ", ` `).Replace(pattern)
}

// regExpEscape escapes the characters of `s` a RegExp reads as syntax.
func regExpEscape(s string) string {
	var b strings.Builder
	for _, r := range s {
		if strings.ContainsRune(`/\^$*+?.()|[]{}`, r) {
			b.WriteByte('\\')
		}
		b.WriteRune(r)
	}
	return b.String()
}

// literalCheck is the check of a string containing a literal at a place:
// `anchor` puts the escaped literal in its pattern.
//
// shortcut: the JS target drops a lone surrogate at the cut end of the
// literal from the JSON Schema pattern and tests UTF-16 units; here the
// pattern keeps it and the test is on bytes, the same unless the literal
// holds a lone surrogate; port the cut if a program checks one
func literalCheck(what, literal string, test func(s string) bool, anchor func(string) string) *check {
	source := regExpSource(anchor(regExpEscape(literal)))
	return &check{
		test:     func(_ *Fiber, v Value) bool { return test(v.(string)) },
		expected: "a string " + what + " " + QuoteJSON(literal),
		keywords: func(string) (*Record, bool) {
			if literal == "" {
				return newRecord(0), true
			}
			return record(Rec("pattern", source)), true
		},
		arb: &arbConstraint{patterns: []string{source}},
	}
}

func CheckStartsWith(prefix Value) Value {
	p := prefix.(string)
	return literalCheck("starting with", p, func(s string) bool { return strings.HasPrefix(s, p) },
		func(e string) string { return "^" + e })
}

func CheckEndsWith(suffix Value) Value {
	p := suffix.(string)
	return literalCheck("ending with", p, func(s string) bool { return strings.HasSuffix(s, p) },
		func(e string) string { return e + "$" })
}

func CheckIncludes(text Value) Value {
	p := text.(string)
	return literalCheck("including", p, func(s string) bool { return strings.Contains(s, p) },
		func(e string) string { return e })
}

// caseCheck is a check that `s` maps to itself; its pattern approximates
// it in JSON Schema.
func caseCheck(expected, pattern string, same func(s string) bool) *check {
	return &check{
		test:     func(_ *Fiber, v Value) bool { return same(v.(string)) },
		expected: expected,
		keywords: func(string) (*Record, bool) { return record(Rec("pattern", pattern)), false },
		arb:      &arbConstraint{patterns: []string{pattern}},
	}
}

// firstUnit is JS `s.charAt(0)`: the first UTF-16 unit, a lone surrogate
// as itself.
func firstUnit(s string) string {
	if s == "" {
		return ""
	}
	return fromUnits(utf16Units(s)[:1])
}

var (
	CheckTrimmed Value = func() *check {
		c := isTrimmed()
		c.arb = &arbConstraint{patterns: []string{`^\S[\s\S]*\S$|^\S$|^$`}}
		return c
	}()
	CheckLowercased Value = caseCheck("a string with all characters in lowercase", "^[^A-Z]*$",
		func(s string) bool { return ToLowerCase(s) == s })
	CheckUppercased Value = caseCheck("a string with all characters in uppercase", "^[^a-z]*$",
		func(s string) bool { return ToUpperCase(s) == s })
	CheckCapitalized Value = caseCheck("a string with the first character in uppercase", `^(?:[^a-z][\s\S]*)?$`,
		func(s string) bool { c := firstUnit(s); return ToUpperCase(c) == c })
	CheckUncapitalized Value = caseCheck("a string with the first character in lowercase", `^(?:[^A-Z][\s\S]*)?$`,
		func(s string) bool { c := firstUnit(s); return ToLowerCase(c) == c })
)

// patternCheck is the check of a JS pattern, run as Go's, with its own
// expected text (the RegExp's by default) and keywords.
func patternCheck(source, expected string, keywords ...Value) *check {
	re := regexp.MustCompile(goPattern(utf16Units(source)))
	if expected == "" {
		expected = "a string matching the RegExp " + source
	}
	return &check{
		test:     func(_ *Fiber, v Value) bool { return re.MatchString(v.(string)) },
		expected: expected,
		keywords: exactly(append([]Value{"pattern", source}, keywords...)...),
		arb:      &arbConstraint{patterns: []string{source}},
	}
}

// CheckUUID is `Check.uuid`: of the version given, a version of 0 read as
// none.
func CheckUUID(version Value) Value {
	v := 0
	if n, ok := some(version); ok {
		v = int(n.(int32))
	}
	if v == 0 {
		return patternCheck(`^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|[fF]{8}-[fF]{4}-[fF]{4}-[fF]{4}-[fF]{12})$`,
			"a UUID", "format", "uuid")
	}
	n := strconv.Itoa(v)
	return patternCheck(`^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-`+n+`[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12})$`,
		"a UUID v"+n, "format", "uuid")
}

var (
	CheckULID         Value = patternCheck(`^[0-7][0-9A-HJKMNP-TV-Za-hjkmnp-tv-z]{25}$`, "")
	CheckGUID         Value = patternCheck(`^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$`, "a GUID")
	CheckBase64       Value = patternCheck(`^([0-9a-zA-Z+/]{4})*(([0-9a-zA-Z+/]{2}==)|([0-9a-zA-Z+/]{3}=))?$`, "a base64 encoded string")
	CheckBase64Url    Value = patternCheck(`^([0-9a-zA-Z-_]{4})*(([0-9a-zA-Z-_]{2}(==)?)|([0-9a-zA-Z-_]{3}(=)?))?$`, "a base64url encoded string")
	CheckStringFinite Value = patternCheck(`^[+-]?\d*\.?\d+(?:[Ee][+-]?\d+)?$`, "a string representing a finite number")
	CheckStringBigInt Value = patternCheck(`^-?\d+$`, "a string representing a bigint")
)
