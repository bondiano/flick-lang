package flickrt

import "strconv"

// The property checks, on the object of a `Codec.entries` codec as the JS
// adapter puts them: before it becomes the list of its entries.

// checkedEntries is an entries codec with checks of its object.
type checkedEntries struct {
	*entriesCodec
	checks []*check
}

// withPropertyCheck is `codec` with `c` on its object; a codec not made by
// `Codec.entries` has no object to check.
func withPropertyCheck(c *check, codec Value) Value {
	switch e := codec.(type) {
	case *entriesCodec:
		return &checkedEntries{e, []*check{c}}
	case *checkedEntries:
		return &checkedEntries{e.entriesCodec, append(append([]*check{}, e.checks...), c)}
	}
	panic(Defect{"TypeError: a property check applies to a codec made by `Codec.entries`"})
}

func (e *checkedEntries) run(fb *Fiber, obj Value) []issue {
	for _, k := range e.checks {
		if issues := k.issues(fb, obj); issues != nil {
			return issues
		}
	}
	return nil
}

func (e *checkedEntries) decode(fb *Fiber, in Value) (Value, []issue) {
	v, issues := e.entriesCodec.decode(fb, in)
	if issues != nil {
		return nil, issues
	}
	return v, e.run(fb, in)
}

// encode checks the object of the entries before encoding its values: the
// decoded side is checked first.
func (e *checkedEntries) encode(fb *Fiber, v Value) (Value, []issue) {
	obj := newRecord(Len(v))
	for _, entry := range List(v) {
		obj.put(At(entry, 0).(string), At(entry, 1))
	}
	if issues := e.run(fb, obj); issues != nil {
		return nil, issues
	}
	return e.entriesCodec.encode(fb, v)
}

func (e *checkedEntries) jsonSchema(x *schemaExport, path string) Value {
	out := record(e.entriesCodec.jsonSchema(x, path))
	for _, k := range e.checks {
		kw, exact := k.keywords("object")
		if !exact {
			refuseExport(path, `the check "`+k.describe()+`" has no exact JSON Schema form`)
		}
		out = mergeKeywords(out, kw)
	}
	return out
}

func entryCount(n int) string {
	if n == 1 {
		return "1 entry"
	}
	return strconv.Itoa(n) + " entries"
}

func keyCount(v Value) int { return len(record(v).keys) }

// propertyCheck is a check of the object's keys, exact on an object.
func propertyCheck(test func(n int) bool, expected string, kw ...Value) *check {
	return &check{
		test:     func(_ *Fiber, v Value) bool { return test(keyCount(v)) },
		expected: expected,
		keywords: func(typ string) (*Record, bool) { return record(Rec(kw...)), typ == "object" },
	}
}

func CheckMinProperties(n, codec Value) Value {
	m := cardinality(n)
	return withPropertyCheck(propertyCheck(func(k int) bool { return k >= m },
		"a value with at least "+entryCount(m), "minProperties", float64(m)), codec)
}

func CheckMaxProperties(n, codec Value) Value {
	m := cardinality(n)
	return withPropertyCheck(propertyCheck(func(k int) bool { return k <= m },
		"a value with at most "+entryCount(m), "maxProperties", float64(m)), codec)
}

func CheckPropertiesBetween(minimum, maximum, codec Value) Value {
	low, high := cardinality(minimum), cardinality(maximum)
	expected := "a value with between " + strconv.Itoa(low) + " and " + strconv.Itoa(high) + " entries"
	if low == high {
		expected = "a value with exactly " + entryCount(low)
	}
	return withPropertyCheck(propertyCheck(func(k int) bool { return k >= low && k <= high },
		expected, "minProperties", float64(low), "maxProperties", float64(high)), codec)
}

// CheckPropertyNames is the check that every key decodes with `key`. The
// JS exporter writes its `propertyNames` without the key's schema, which
// JSON drops, so it states nothing.
//
// shortcut: a key runs through the whole codec, where the JS target checks
// it against the codec's encoded side only; the same for codecs without a
// transformation; check the encoded side if a program names keys by one
func CheckPropertyNames(key, codec Value) Value {
	k := codecOf(key)
	return withPropertyCheck(&check{
		test: func(fb *Fiber, v Value) bool {
			for _, name := range record(v).keys {
				if _, issues := k.decode(fb, name); issues != nil {
					return false
				}
			}
			return true
		},
		expected: "an object with property names matching the schema",
		keywords: func(typ string) (*Record, bool) { return newRecord(0), typ == "object" },
	}, codec)
}
