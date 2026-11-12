package flickrt

import (
	"math"
	"math/big"
	"regexp"
)

// The schemas and JSON codecs of the standard library's types: durations,
// decimals, chunks, hash sets and hash maps, as the JS runtime builds them.

// CodecBigint is `BigInt.json`: a string of digits.
var CodecBigint Value = sch(SchemaBigint).jsonOf()

// isPattern is the check of a regular expression without flags, which its
// JSON Schema states only approximately.
func isPattern(source string) *check {
	re := regexp.MustCompile(source)
	return &check{
		test:     func(_ *Fiber, v Value) bool { return re.MatchString(v.(string)) },
		expected: "a string matching the RegExp " + source,
		keywords: func(string) (*Record, bool) { return newRecord(0), false },
	}
}

var nonNegativeDuration = &check{
	test:     func(_ *Fiber, v Value) bool { return !dur(v).negative() },
	expected: "a non-negative duration",
}

// toMillis is the milliseconds of a duration: an infinity as itself.
func toMillis(d Value) Value {
	switch x := dur(d); x.tag {
	case durationInfinite:
		return math.Inf(1)
	case durationNegativeInfinite:
		return math.Inf(-1)
	}
	return value(DurationToMillis(d))
}

// String is the text of a duration, as JS `String(duration)` writes it.
func (d *Duration) String() string {
	switch d.tag {
	case durationInfinite:
		return "Infinity"
	case durationNegativeInfinite:
		return "-Infinity"
	case durationNanos:
		return d.nanos.String() + " nanos"
	}
	return jsString(d.millis) + " millis"
}

var (
	// CodecDurationMillis is `Duration.json_millis`: a finite, non-negative
	// number of milliseconds.
	CodecDurationMillis Value = &stage{
		withChecks(CodecNumberFinite.(codec), isGreaterThanOrEqualTo(0)),
		pure(func(v Value) Value { return fromMillis(v.(float64)) }),
		pure(toMillis),
	}

	// CodecDurationNanos is `Duration.json_nanos`: nanoseconds as a string
	// of digits; an infinite duration has none.
	CodecDurationNanos Value = &stage{
		withChecks(CodecString.(codec), isPattern(`^[0-9]+$`)),
		pure(func(v Value) Value { n, _ := new(big.Int).SetString(v.(string), 10); return fromNanos(n) }),
		func(_ *Fiber, d Value) (Value, []issue) {
			if dur(d).infinite() {
				return nil, fail("an invalid value")
			}
			return dur(d).toNanos().String(), nil
		},
	}

	// CodecDurationString is `Duration.json_string`: the text form of a
	// duration.
	CodecDurationString Value = &fallible{withChecks(&stage{
		&stringAs{"a string that will be decoded as a Duration"},
		func(_ *Fiber, v Value) (Value, []issue) {
			// Effect's `Duration.fromInput` also reads "-Infinity", which the check refuses
			if v.(string) == "-Infinity" {
				return &Duration{tag: durationNegativeInfinite}, nil
			}
			if d, ok := parseDuration(v.(string)); ok {
				return d, nil
			}
			return nil, fail("an invalid value")
		},
		pure(func(d Value) Value { return dur(d).String() }),
	}, nonNegativeDuration)}

	SchemaDuration Value = needing("a check", customSchema(CodecDurationMillis.(codec), func(fb *Fiber, v Value) (Value, []issue) {
		if _, ok := v.(*Duration); !ok {
			return nil, expected("Duration", v)
		}
		return v, nonNegativeDuration.issues(fb, v)
	}))

	// CodecDecimal is `BigDecimal.json`: a decimal as its string.
	CodecDecimal Value = &fallible{&stage{
		&stringAs{"a string that will be decoded as a BigDecimal"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if r := DecimalFromString(v); isOk(r) {
				return value(r), nil
			}
			return nil, fail("an invalid value")
		},
		pure(DecimalToString),
	}}

	SchemaDecimal Value = needing("BigDecimal", customSchema(CodecDecimal.(codec), func(_ *Fiber, v Value) (Value, []issue) {
		if _, ok := v.(*Decimal); !ok {
			return nil, expected("BigDecimal", v)
		}
		return v, nil
	}))
)

// Chunk is an immutable chunk: its elements, never changed.
type Chunk struct{ items []Value }

var ChunkEmpty Value = &Chunk{}

func ChunkFromList(xs Value) Value { return &Chunk{List(xs)} }
func ChunkToList(c Value) Value    { return c.(*Chunk).items }
func ChunkSize(c Value) Value      { return int32(len(c.(*Chunk).items)) }

// CodecChunk is `Chunk.json`: a chunk as a JSON array.
func CodecChunk(item Value) Value {
	return linkCodec(&stage{&listCodec{codecOf(item)}, pure(ChunkFromList), pure(ChunkToList)},
		func(b *arbASTs) *arbAST { return chunkNode(b.ofCodec(codecOf(item))) })
}

// SchemaChunk is `Schema.chunk`; its canonical JSON form is an array.
func SchemaChunk(item Value) Value {
	s := sch(item)
	out := needing("Chunk", customSchema(codecOf(CodecChunk(s.jsonOf())), func(fb *Fiber, v Value) (Value, []issue) {
		c, ok := v.(*Chunk)
		if !ok {
			return nil, expected("Chunk", v)
		}
		xs, issues := validateAll(fb, c.items, func(int) *Schema { return s })
		if issues != nil {
			return nil, issues
		}
		return ChunkFromList(xs), nil
	}))
	sch(out).item = s // what Schema.arbitrary generates the elements of
	return out
}

// CodecHashSet is `HashSet.json`: a set as a JSON array of its elements,
// rebuilt with the canonical witness of the `Hash` dictionary.
func CodecHashSet(fb *Fiber, d, item Value) Value {
	w := EqHashCanonical(fb, d)
	return &stage{
		&listCodec{codecOf(item)},
		func(fb *Fiber, xs Value) (Value, []issue) { return HashSetFromList(fb, w, xs), nil },
		pure(HashSetToList),
	}
}

// CodecHashMap is `HashMap.json`: a map as a JSON array of `[key, value]`.
func CodecHashMap(fb *Fiber, d, key, val Value) Value {
	w := EqHashCanonical(fb, d)
	return &stage{
		&listCodec{&pair{codecOf(key), codecOf(val)}},
		func(fb *Fiber, entries Value) (Value, []issue) { return HashMapFromList(fb, w, entries), nil },
		pure(HashMapToList),
	}
}

// pair is a JSON array of exactly two values, each of its own codec.
type pair struct{ first, second codec }

func (p *pair) accepts(in Value) bool { _, ok := in.([]Value); return ok }

func (p *pair) decode(fb *Fiber, in Value) (Value, []issue) {
	xs, ok := in.([]Value)
	if !ok {
		return nil, expected("an array", in)
	}
	return p.each(xs, func(c codec, x Value) (Value, []issue) { return c.decode(fb, x) })
}

func (p *pair) encode(fb *Fiber, v Value) (Value, []issue) {
	return p.each(List(v), func(c codec, x Value) (Value, []issue) { return c.encode(fb, x) })
}

func (p *pair) each(xs []Value, f func(codec, Value) (Value, []issue)) (Value, []issue) {
	out := make([]Value, 2)
	for i, c := range []codec{p.first, p.second} {
		if i >= len(xs) {
			return nil, under(i, fail("a missing key"))
		}
		v, issues := f(c, xs[i])
		if issues != nil {
			return nil, under(i, issues)
		}
		out[i] = v
	}
	if len(xs) > 2 {
		return nil, under(2, fail("an unknown key"))
	}
	return out, nil
}

func (p *pair) jsonSchema(x *schemaExport, path string) Value {
	return Rec("type", "array", "prefixItems", []Value{x.codec(p.first, path+"[0]"), x.codec(p.second, path+"[1]")}, "items", false)
}
