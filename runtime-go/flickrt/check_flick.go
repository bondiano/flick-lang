package flickrt

import (
	"math"
	"regexp"
)

// The checks of the standard library's `Check` that state what values
// Arbitrary generates: Effect's filters with their arbitrary constraints.
// The rest are in check_strings.go, check_order.go and check_properties.go.

// CheckApply is `Check.apply`.
func CheckApply(c, codec Value) Value { return withChecks(codecOf(codec), checksOf(c)...) }

func bound(v float64) *float64 { return &v }

// CheckInt is `Check.int`: a safe integer.
var CheckInt Value = func() *check {
	c := isInt()
	c.arb = &arbConstraint{number: "integer"}
	return c
}()

// CheckFinite is `Check.finite`.
var CheckFinite Value = func() *check {
	c := isFinite()
	c.arb = &arbConstraint{number: "finite"}
	return c
}()

// CheckBetween is `Check.between`, each bound excluded by its flag.
func CheckBetween(options Value) Value {
	o := record(options)
	minimum, maximum := o.Get("minimum").(float64), o.Get("maximum").(float64)
	exclusiveMin, exclusiveMax := o.Get("exclusiveMinimum").(bool), o.Get("exclusiveMaximum").(bool)
	excluded := func(b bool) string {
		if b {
			return " (excluded)"
		}
		return ""
	}
	low, high := "minimum", "maximum"
	if exclusiveMin {
		low = "exclusiveMinimum"
	}
	if exclusiveMax {
		high = "exclusiveMaximum"
	}
	return &check{
		test: func(_ *Fiber, v Value) bool {
			x := numberOf(v)
			return (x > minimum || !exclusiveMin && x == minimum) && (x < maximum || !exclusiveMax && x == maximum)
		},
		expected: "a value between " + formatNumber(minimum) + excluded(exclusiveMin) + " and " +
			formatNumber(maximum) + excluded(exclusiveMax),
		keywords: exactly(low, minimum, high, maximum),
		arb: &arbConstraint{order: "number", minimum: minimum, maximum: maximum,
			exclusiveMinimum: exclusiveMin, exclusiveMaximum: exclusiveMax},
	}
}

// cardinality is Effect's `normalizeCardinality`: a whole count, 0 at least.
func cardinality(n Value) int {
	return int(math.Max(0, math.Floor(float64(n.(int32)))))
}

// CheckMinLength is `Check.min_length`.
func CheckMinLength(n Value) Value {
	c := isMinLength(cardinality(n))
	c.arb = &arbConstraint{minLength: bound(float64(cardinality(n)))}
	return c
}

// CheckMaxLength is `Check.max_length`.
func CheckMaxLength(n Value) Value {
	maximum := cardinality(n)
	return &check{
		test:     func(_ *Fiber, v Value) bool { return lengthOf(v) <= maximum },
		expected: "a value with a length of at most " + jsString(float64(maximum)),
		keywords: func(typ string) (*Record, bool) {
			switch typ {
			case "string":
				return record(Rec("maxLength", float64(maximum))), maximum == 0
			case "array":
				return record(Rec("maxItems", float64(maximum))), true
			}
			return newRecord(0), false
		},
		arb: &arbConstraint{maxLength: bound(float64(maximum))},
	}
}

// CheckPattern is `Check.pattern`: a JS regular expression without flags,
// run as Go's.
//
// shortcut: a pattern Go's syntax lacks (lookarounds, backreferences) is a defect; port a JS matcher if a program checks one
func CheckPattern(source Value) Value {
	src := source.(string)
	re, err := regexp.Compile(goPattern(utf16Units(src)))
	if err != nil {
		panic(Defect{"SyntaxError: Invalid regular expression: /" + src + "/: " + err.Error()})
	}
	return &check{
		test:     func(_ *Fiber, v Value) bool { return re.MatchString(v.(string)) },
		expected: "a string matching the RegExp " + src,
		keywords: func(string) (*Record, bool) { return newRecord(0), false },
		arb:      &arbConstraint{patterns: []string{src}},
	}
}
