package flickrt

import "math"

// Checks: Effect Schema's filters as values. A check tests a decoded value,
// says what an issue reports when it fails, and carries the JSON Schema
// keywords that state it, as Effect's `toJsonSchema` annotation does, so the
// strict exporter writes it exactly or refuses it.
type check struct {
	test func(fb *Fiber, v Value) bool
	// expected names what passes: "expected <expected>, got …"
	expected string
	// message, when set, is the whole message of a failure
	message string
	// keywords is the check in JSON Schema for a node of JSON type `typ`
	// ("" when unknown); exact is false when they only approximate it.
	// nil: it has no JSON Schema form.
	keywords func(typ string) (kw *Record, exact bool)
	// arb is what generating values of the checked type reads of it
	arb *arbConstraint
}

// issues is the issue of `v` failing the check, or nil.
func (c *check) issues(fb *Fiber, v Value) []issue {
	switch {
	case c.test(fb, v):
		return nil
	case c.message != "":
		return fail(c.message)
	case c.expected != "":
		return expected(c.expected, v)
	}
	return fail("an invalid value")
}

// describe is how an ExportError names the check.
func (c *check) describe() string {
	switch {
	case c.expected != "":
		return c.expected
	case c.message != "":
		return c.message
	}
	return "a check"
}

// exactly is keywords that state a check exactly for every JSON type.
func exactly(kw ...Value) func(string) (*Record, bool) {
	return func(string) (*Record, bool) { return record(Rec(kw...)), true }
}

// formatNumber is Effect's `format` of a number in an issue.
func formatNumber(n float64) string { return jsString(n) }

func numberOf(v Value) float64 { x, _ := number(v); return x }

// isInt is Effect's `isInt`: a safe integer.
func isInt() *check {
	return &check{
		test: func(_ *Fiber, v Value) bool {
			x := numberOf(v)
			return x == math.Trunc(x) && math.Abs(x) <= maxSafe
		},
		expected: "an integer",
		keywords: exactly("type", "integer", "minimum", float64(-maxSafe), "maximum", float64(maxSafe)),
	}
}

// isFinite is Effect's `isFinite`.
func isFinite() *check {
	return &check{
		test:     func(_ *Fiber, v Value) bool { x := numberOf(v); return !math.IsNaN(x) && !math.IsInf(x, 0) },
		expected: "a finite number",
		keywords: exactly("type", "number"),
	}
}

func isGreaterThanOrEqualTo(minimum float64) *check {
	return &check{
		test:     func(_ *Fiber, v Value) bool { return numberOf(v) >= minimum },
		expected: "a value greater than or equal to " + formatNumber(minimum),
		keywords: exactly("minimum", minimum),
	}
}

// isBetween is Effect's `isBetween` of numbers, inclusive; `expected`
// overrides its text when set.
func isBetween(minimum, maximum float64, expected string) *check {
	if expected == "" {
		expected = "a value between " + formatNumber(minimum) + " and " + formatNumber(maximum)
	}
	return &check{
		test:     func(_ *Fiber, v Value) bool { x := numberOf(v); return x >= minimum && x <= maximum },
		expected: expected,
		keywords: exactly("minimum", minimum, "maximum", maximum),
	}
}

// isBetweenLength is Effect's `isBetweenLength` of a string, in UTF-16
// units, or of a list.
func isBetweenLength(minimum, maximum int) *check {
	expected := "a value with a length between " + jsString(float64(minimum)) + " and " + jsString(float64(maximum))
	if minimum == maximum {
		expected = "a value with a length of " + jsString(float64(minimum))
	}
	return &check{
		test: func(_ *Fiber, v Value) bool {
			n := lengthOf(v)
			return n >= minimum && n <= maximum
		},
		expected: expected,
		keywords: func(typ string) (*Record, bool) {
			// a code point may take two UTF-16 units, which JSON Schema counts as one
			half := float64((minimum + 1) / 2)
			switch typ {
			case "string":
				kw := record(Rec("minLength", half, "maxLength", float64(maximum)))
				return kw, maximum == 0 || (minimum+1)/2 > maximum
			case "array":
				return record(Rec("minItems", float64(minimum), "maxItems", float64(maximum))), true
			}
			return newRecord(0), false
		},
	}
}

// lengthOf is the length Effect's length checks test: a string's in UTF-16
// units, or a list's.
func lengthOf(v Value) int {
	if s, ok := v.(string); ok {
		return len(utf16Units(s))
	}
	return len(List(v))
}

// isMinLength is Effect's `isMinLength` (`isNonEmpty` is of 1).
func isMinLength(minimum int) *check {
	return &check{
		test:     func(_ *Fiber, v Value) bool { return lengthOf(v) >= minimum },
		expected: "a value with a length of at least " + jsString(float64(minimum)),
		keywords: func(typ string) (*Record, bool) {
			switch typ {
			case "string":
				// a code point may take two UTF-16 units, which JSON Schema counts as one
				return record(Rec("minLength", float64((minimum+1)/2))), minimum <= 1
			case "array":
				return record(Rec("minItems", float64(minimum))), true
			}
			return newRecord(0), false
		},
	}
}

// isTrimmed is Effect's `isTrimmed`.
func isTrimmed() *check {
	return &check{
		test:     func(_ *Fiber, v Value) bool { return Trim(v) == v },
		expected: "a string with no leading or trailing whitespace",
		keywords: exactly("pattern", `^\S[\s\S]*\S$|^\S$|^$`),
	}
}

// Bounds that combine into one: the tighter of the two.
var (
	lowerBounds = map[string]bool{"minimum": true, "exclusiveMinimum": true, "minLength": true, "minItems": true, "minProperties": true}
	upperBounds = map[string]bool{"maximum": true, "exclusiveMaximum": true, "maxLength": true, "maxItems": true, "maxProperties": true}
)

// mergeKeywords is `out` with `kw` added: a keyword already there is
// tightened, or joins the others under `allOf`.
func mergeKeywords(out, kw *Record) *Record {
	for i, k := range kw.keys {
		v := kw.vals[i]
		at := out.index(k)
		if at < 0 {
			out = out.set(k, v)
			continue
		}
		old := out.vals[at]
		oldN, oldIsNumber := old.(float64)
		n, isNumber := v.(float64)
		switch {
		// an integer is a number: the narrower type replaces the wider
		case k == "type" && old == "number" && v == "integer":
			out = out.set(k, v)
		case oldIsNumber && isNumber && lowerBounds[k]:
			out = out.set(k, math.Max(oldN, n))
		case oldIsNumber && isNumber && upperBounds[k]:
			out = out.set(k, math.Min(oldN, n))
		case JsonStringify(old) != JsonStringify(v):
			all, _ := recordField(out, "allOf").([]Value)
			out = out.set("allOf", append(append([]Value{}, all...), Rec(k, v)))
		}
	}
	return out
}

// checkedCodec is a codec with checks on what it decodes and encodes, and the
// title and description `Codec.annotate` gives it (Options).
type checkedCodec struct {
	from        codec
	checks      []*check
	title       Value
	description Value
}

// withChecks is the codec with more checks; as Effect puts annotations on
// the last check, a new check has none.
func withChecks(c codec, checks ...*check) *checkedCodec {
	if k, ok := c.(*checkedCodec); ok {
		return &checkedCodec{k.from, append(append([]*check{}, k.checks...), checks...), None, None}
	}
	return &checkedCodec{c, checks, None, None}
}

func (c *checkedCodec) accepts(in Value) bool { return c.from.accepts(in) }

func (c *checkedCodec) decode(fb *Fiber, in Value) (Value, []issue) {
	v, issues := c.from.decode(fb, in)
	if issues != nil {
		return nil, issues
	}
	return v, c.run(fb, v)
}

func (c *checkedCodec) encode(fb *Fiber, v Value) (Value, []issue) {
	if issues := c.run(fb, v); issues != nil {
		return nil, issues
	}
	return c.from.encode(fb, v)
}

// run stops at the first check that fails, as Effect's default does.
func (c *checkedCodec) run(fb *Fiber, v Value) []issue {
	for _, k := range c.checks {
		if issues := k.issues(fb, v); issues != nil {
			return issues
		}
	}
	return nil
}

func (c *checkedCodec) jsonSchema(x *schemaExport, path string) Value {
	base := x.codec(c.from, path)
	out, ok := base.(*Record)
	typ := ""
	if ok {
		typ, _ = recordField(out, "type").(string)
	}
	kw := newRecord(0)
	for _, k := range c.checks {
		if k.keywords == nil {
			refuseExport(path, `the check "`+k.describe()+`" has no JSON Schema form`)
		}
		// a check of a transformed value constrains the decoded side, not the wire
		if transforms(c.from) {
			refuseExport(path, `the check "`+k.describe()+`" applies after a transformation`)
		}
		more, exact := k.keywords(typ)
		if !exact {
			refuseExport(path, `the check "`+k.describe()+`" has no exact JSON Schema form`)
		}
		kw = mergeKeywords(kw, more)
	}
	if !ok {
		return base
	}
	if t, isSome := some(c.title); isSome {
		out = out.set("title", t)
	}
	if d, isSome := some(c.description); isSome {
		out = out.set("description", d)
	}
	return mergeKeywords(out, kw)
}

// transforms is whether decoding the codec changes the wire value, so a
// check after it is not one of the wire.
func transforms(c codec) bool {
	switch k := c.(type) {
	case *checkedCodec:
		return transforms(k.from)
	case *wire:
		switch s := k.s.resolved(); s.kind {
		case kindNumber, kindInt, kindString, kindBool, kindList, kindTuple, kindRecord, kindNever, kindUnknown:
			return false
		case kindRefined:
			return transforms(s.item.jsonOf())
		}
	// an array's node has no encoding of its own, whatever its items have
	case *literalCodec, *numberAs, *stringAs, jsonValue, *listCodec:
		return false
	}
	return true
}

// CodecAnnotate is `Codec.annotate`: None keeps what the codec had.
func CodecAnnotate(annotations, c Value) Value {
	r := record(annotations)
	k, ok := c.(*checkedCodec)
	if !ok {
		k = &checkedCodec{codecOf(c), nil, None, None}
	}
	out := *k
	if t := r.Get("title"); isSome(t) {
		out.title = t
	}
	if d := r.Get("description"); isSome(d) {
		out.description = d
	}
	return &out
}

// CodecAnnotations is `Codec.annotations`.
func CodecAnnotations(c Value) Value {
	if k, ok := c.(*checkedCodec); ok {
		return Rec("title", k.title, "description", k.description)
	}
	return Rec("title", None, "description", None)
}
