package flickrt

import (
	"math"
	"math/big"
	"strconv"
)

// The checks of `Check` on ordered values and collections: the bounds of
// an order, and the size, uniqueness and property checks.

// orderChecks are the bound checks of an order: greater than, less than
// and their kin, formatting a bound with `format`.
type orderChecks struct {
	compare  func(fb *Fiber, a, b Value) int
	format   func(Value) string
	keywords func(kw ...Value) func(string) (*Record, bool)
	// arbOrder is the order arbitrary generation knows ("" for none)
	arbOrder string
}

func (o orderChecks) check(test func(int) bool, bound Value, expected string, arb *arbConstraint, kw ...Value) *check {
	c := &check{
		test:     func(fb *Fiber, v Value) bool { return test(o.compare(fb, v, bound)) },
		expected: expected + " " + o.format(bound),
	}
	if o.keywords != nil {
		c.keywords = o.keywords(kw...)
	}
	if o.arbOrder != "" {
		arb.order = o.arbOrder
		c.arb = arb
	}
	return c
}

func (o orderChecks) greaterThan(b Value) Value {
	return o.check(func(c int) bool { return c > 0 }, b, "a value greater than",
		&arbConstraint{minimum: b, exclusiveMinimum: true}, "exclusiveMinimum", b)
}

func (o orderChecks) greaterThanOrEqualTo(b Value) Value {
	return o.check(func(c int) bool { return c >= 0 }, b, "a value greater than or equal to",
		&arbConstraint{minimum: b}, "minimum", b)
}

func (o orderChecks) lessThan(b Value) Value {
	return o.check(func(c int) bool { return c < 0 }, b, "a value less than",
		&arbConstraint{maximum: b, exclusiveMaximum: true}, "exclusiveMaximum", b)
}

func (o orderChecks) lessThanOrEqualTo(b Value) Value {
	return o.check(func(c int) bool { return c <= 0 }, b, "a value less than or equal to",
		&arbConstraint{maximum: b}, "maximum", b)
}

// between is the check of a value between the bounds of `{ minimum,
// maximum, exclusiveMinimum, exclusiveMaximum }`.
func (o orderChecks) between(options Value) Value {
	r := record(options)
	low, high := r.Get("minimum"), r.Get("maximum")
	exMin, exMax := Bool(r.Get("exclusiveMinimum")), Bool(r.Get("exclusiveMaximum"))
	excluded := func(b bool) string {
		if b {
			return " (excluded)"
		}
		return ""
	}
	c := &check{
		test: func(fb *Fiber, v Value) bool {
			l, h := o.compare(fb, v, low), o.compare(fb, v, high)
			return (l > 0 || !exMin && l == 0) && (h < 0 || !exMax && h == 0)
		},
		expected: "a value between " + o.format(low) + excluded(exMin) + " and " + o.format(high) + excluded(exMax),
	}
	if o.keywords != nil {
		c.keywords = o.keywords()
	}
	if o.arbOrder != "" {
		c.arb = &arbConstraint{order: o.arbOrder, minimum: low, maximum: high, exclusiveMinimum: exMin, exclusiveMaximum: exMax}
	}
	return c
}

// finiteBound is a bound of a number check, which must be finite.
func finiteBound(b Value) Value {
	if x := b.(float64); math.IsNaN(x) || math.IsInf(x, 0) {
		panic(Defect{"RangeError: Expected a finite number, got " + formatNumber(x)})
	}
	return b
}

// approximately is keywords that state a check only in part: none exact.
func approximately(...Value) func(string) (*Record, bool) {
	return func(string) (*Record, bool) { return newRecord(0), false }
}

// numberCompare is the order of numbers: NaN below every number.
func numberCompare(a, b float64) int {
	switch {
	case a == b || a != a && b != b:
		return 0
	case a != a || a < b:
		return -1
	}
	return 1
}

var (
	numberChecks = orderChecks{
		compare:  func(_ *Fiber, a, b Value) int { return numberCompare(numberOf(a), b.(float64)) },
		format:   func(b Value) string { return formatNumber(b.(float64)) },
		keywords: exactly,
		arbOrder: "number",
	}
	bigintChecks = orderChecks{
		compare:  func(_ *Fiber, a, b Value) int { return bigOrder(a, b) },
		format:   func(b Value) string { return b.(*big.Int).String() + "n" },
		keywords: approximately,
		arbOrder: "bigint",
	}
	decimalChecks = orderChecks{
		compare: func(_ *Fiber, a, b Value) int { return decCompare(dec(a), dec(b)) },
		format:  func(b Value) string { return DecimalToString(b).(string) },
	}
)

func CheckGreaterThan(b Value) Value { return numberChecks.greaterThan(finiteBound(b)) }
func CheckGreaterThanOrEqualTo(b Value) Value {
	return numberChecks.greaterThanOrEqualTo(finiteBound(b))
}
func CheckLessThan(b Value) Value          { return numberChecks.lessThan(finiteBound(b)) }
func CheckLessThanOrEqualTo(b Value) Value { return numberChecks.lessThanOrEqualTo(finiteBound(b)) }

// CheckMultipleOf is the check of a multiple of the divisor's magnitude,
// by `Number.remainder`.
func CheckMultipleOf(divisor Value) Value {
	d := divisor.(float64)
	if math.IsNaN(d) || math.IsInf(d, 0) || d == 0 {
		panic(Defect{"RangeError: Expected a finite non-zero number, got " + formatNumber(d)})
	}
	d = math.Abs(d)
	return &check{
		test:     func(_ *Fiber, v Value) bool { return NumberRemainder(d, numberOf(v)).(float64) == 0 },
		expected: "a value that is a multiple of " + formatNumber(d),
		keywords: exactly("multipleOf", d),
	}
}

// checkIntBetween is the group of `isInt` and `isBetween`.
func checkIntBetween(minimum, maximum float64) Value {
	return checkGroup{CheckInt.(*check), CheckBetween(Rec("minimum", minimum, "maximum", maximum,
		"exclusiveMinimum", false, "exclusiveMaximum", false)).(*check)}
}

var (
	CheckInt32  = checkIntBetween(-2147483648, 2147483647)
	CheckUint32 = checkIntBetween(0, 4294967295)
)

func CheckGreaterThanBigInt(b Value) Value          { return bigintChecks.greaterThan(b) }
func CheckGreaterThanOrEqualToBigInt(b Value) Value { return bigintChecks.greaterThanOrEqualTo(b) }
func CheckLessThanBigInt(b Value) Value             { return bigintChecks.lessThan(b) }
func CheckLessThanOrEqualToBigInt(b Value) Value    { return bigintChecks.lessThanOrEqualTo(b) }
func CheckBetweenBigInt(options Value) Value        { return bigintChecks.between(options) }

func CheckGreaterThanDecimal(b Value) Value          { return decimalChecks.greaterThan(b) }
func CheckGreaterThanOrEqualToDecimal(b Value) Value { return decimalChecks.greaterThanOrEqualTo(b) }
func CheckLessThanDecimal(b Value) Value             { return decimalChecks.lessThan(b) }
func CheckLessThanOrEqualToDecimal(b Value) Value    { return decimalChecks.lessThanOrEqualTo(b) }
func CheckBetweenDecimal(options Value) Value        { return decimalChecks.between(options) }

// effectFormat is the text of a bound in an expected text.
func effectFormat(v Value) string { return formatSpaced(v, 0) }

// byOrd is the order of a Flick `Ord` dictionary.
func byOrd(d Value) orderChecks {
	return orderChecks{
		compare: func(fb *Fiber, a, b Value) int {
			switch apply2(fb, Method(fb, d, "compare"), a, b).(*Data).Tag {
			case LT.(*Data).Tag:
				return -1
			case EQ.(*Data).Tag:
				return 0
			}
			return 1
		},
		format: effectFormat,
	}
}

func CheckGreaterThanBy(d, b Value) Value          { return byOrd(d).greaterThan(b) }
func CheckGreaterThanOrEqualToBy(d, b Value) Value { return byOrd(d).greaterThanOrEqualTo(b) }
func CheckLessThanBy(d, b Value) Value             { return byOrd(d).lessThan(b) }
func CheckLessThanOrEqualToBy(d, b Value) Value    { return byOrd(d).lessThanOrEqualTo(b) }

func CheckBetweenBy(d, minimum, maximum Value) Value {
	return byOrd(d).between(Rec("minimum", minimum, "maximum", maximum,
		"exclusiveMinimum", false, "exclusiveMaximum", false))
}

// CheckMultipleOfBy is the check of a multiple by a `remainder` of the
// program: the remainder is `zero` by JS `===`.
func CheckMultipleOfBy(remainder, zero, divisor Value) Value {
	return &check{
		test:     func(fb *Fiber, v Value) bool { return Same(apply2(fb, remainder, v, divisor), zero) },
		expected: "a value that is a multiple of " + effectFormat(divisor),
	}
}

// unique is whether no two of `xs` are equal by the structural `==`.
func unique(xs []Value) bool {
	return len(List(ArrayDedupeWith(nil, equalsFn, xs))) == len(xs)
}

// shortcut: the arbitrary constraints of uniqueness, sizes and properties
// are not read on the go target, which filters generated values by the
// check instead; carry them in arbConstraint if a program generates such
// values

var (
	CheckUnique Value = &check{
		test:     func(_ *Fiber, v Value) bool { return unique(List(v)) },
		expected: "an array with unique items",
		keywords: exactly("uniqueItems", true),
	}
	CheckUniqueKeys Value = &check{
		test: func(_ *Fiber, v Value) bool {
			keys := make([]Value, Len(v))
			for i, entry := range List(v) {
				keys[i] = At(entry, 0)
			}
			return unique(keys)
		},
		expected: "an array with unique keys",
	}
)

// sizeCheck is a check of the size of a `HashSet`.
func sizeCheck(test func(n int) bool, expected string) Value {
	return &check{
		test:     func(_ *Fiber, v Value) bool { return test(int(HashSetSize(v).(int32))) },
		expected: expected,
		keywords: approximately(),
	}
}

func CheckMinSize(n Value) Value {
	m := cardinality(n)
	return sizeCheck(func(s int) bool { return s >= m }, "a value with a size of at least "+strconv.Itoa(m))
}

func CheckMaxSize(n Value) Value {
	m := cardinality(n)
	return sizeCheck(func(s int) bool { return s <= m }, "a value with a size of at most "+strconv.Itoa(m))
}

func CheckSizeBetween(minimum, maximum Value) Value {
	low, high := cardinality(minimum), cardinality(maximum)
	expected := "a value with a size between " + strconv.Itoa(low) + " and " + strconv.Itoa(high)
	if low == high {
		expected = "a value with a size of " + strconv.Itoa(low)
	}
	return sizeCheck(func(s int) bool { return s >= low && s <= high }, expected)
}
