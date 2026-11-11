package flickrt

import (
	"math"
	"slices"
	"strings"
)

// Codecs of the `Codec` module: the canonical JSON codec of a schema, and
// codecs built out of codecs.

func SchemaJsonCodec(s Value) Value { return sch(s).jsonOf() }

// JsonCodec is `Json.json`: a JSON value as itself. A tree that holds
// anything else, such as an infinity a parse gave for `1e400`, fails as a
// whole, both ways.
var JsonCodec Value = jsonValue{}

type jsonValue struct{}

func isJSONTree(in Value) bool {
	switch x := in.(type) {
	case *jsonNull, string, bool, int32:
		return true
	case float64:
		return !math.IsNaN(x) && !math.IsInf(x, 0)
	case []Value:
		return !slices.ContainsFunc(x, func(v Value) bool { return !isJSONTree(v) })
	case *Record:
		return !slices.ContainsFunc(x.vals, func(v Value) bool { return !isJSONTree(v) })
	}
	return false
}

func (jsonValue) accepts(in Value) bool { return isJSONTree(in) }

func (jsonValue) decode(_ *Fiber, in Value) (Value, []issue) {
	if !isJSONTree(in) {
		return nil, expected("JSON value", in)
	}
	return in, nil
}

func (j jsonValue) encode(fb *Fiber, v Value) (Value, []issue) { return j.decode(fb, v) }

var CodecBool Value = sch(SchemaBool).jsonOf()

// CodecInt is `Int.json`, an Int's canonical JSON codec.
var CodecInt Value = sch(SchemaInt).jsonOf()

func codecOf(c Value) codec { return c.(codec) }

// CodecNumberLossless is `Float.json`: every number, `NaN`, the
// infinities and `-0` as the strings that name them.
var CodecNumberLossless Value = losslessNumber{}

type losslessNumber struct{}

var losslessNames = map[string]float64{
	"NaN": math.NaN(), "Infinity": math.Inf(1), "-Infinity": math.Inf(-1), "-0": math.Copysign(0, -1),
}

func (losslessNumber) accepts(in Value) bool {
	_, isNumber := in.(float64)
	s, isString := in.(string)
	_, named := losslessNames[s]
	return isNumber || isString && named
}

func (n losslessNumber) decode(_ *Fiber, in Value) (Value, []issue) {
	if !n.accepts(in) {
		return nil, expected("a number", in)
	}
	if s, ok := in.(string); ok {
		return losslessNames[s], nil
	}
	return in, nil
}

func (losslessNumber) encode(_ *Fiber, v Value) (Value, []issue) {
	x := v.(float64)
	switch {
	case math.IsNaN(x):
		return "NaN", nil
	case math.IsInf(x, 1):
		return "Infinity", nil
	case math.IsInf(x, -1):
		return "-Infinity", nil
	case x == 0 && math.Signbit(x):
		return "-0", nil
	}
	return x, nil
}

func CodecDecodeResult(fb *Fiber, c, in Value) Value {
	if problem := inertProblem(in); problem != nil {
		return Err(schemaErrorOf(problem))
	}
	v, issues := codecOf(c).decode(fb, in)
	if issues != nil {
		return Err(schemaErrorOf(issues))
	}
	return Ok(v)
}

func CodecEncodeResult(fb *Fiber, c, v Value) Value {
	e, issues := codecOf(c).encode(fb, v)
	if issues != nil {
		return Err(schemaErrorOf(issues))
	}
	return Ok(e)
}

// CodecDecode and CodecEncode fail with the SchemaError.
func CodecDecode(fb *Fiber, c, in Value) Value { return FromResult(CodecDecodeResult(fb, c, in)) }
func CodecEncode(fb *Fiber, c, v Value) Value  { return FromResult(CodecEncodeResult(fb, c, v)) }

// listCodec is `Codec.list`: a JSON array of the item's wire form.
type listCodec struct{ item codec }

func CodecList(item Value) Value { return &listCodec{codecOf(item)} }

func (l *listCodec) accepts(in Value) bool { _, ok := in.([]Value); return ok }

func (l *listCodec) decode(fb *Fiber, in Value) (Value, []issue) {
	xs, ok := in.([]Value)
	if !ok {
		return nil, expected("an array", in)
	}
	out := make([]Value, len(xs))
	for i, x := range xs {
		v, issues := l.item.decode(fb, x)
		if issues != nil {
			return nil, under(i, issues)
		}
		out[i] = v
	}
	return out, nil
}

func (l *listCodec) encode(fb *Fiber, v Value) (Value, []issue) {
	xs := List(v)
	out := make([]Value, len(xs))
	for i, x := range xs {
		e, issues := l.item.encode(fb, x)
		if issues != nil {
			return nil, under(i, issues)
		}
		out[i] = e
	}
	return out, nil
}

// taggedOption is `Codec.tagged_option`: `{ "_tag": "None" }` or
// `{ "_tag": "Some", "value": … }`.
type taggedOption struct{ item codec }

func CodecTaggedOption(item Value) Value { return &taggedOption{codecOf(item)} }

func (t *taggedOption) accepts(in Value) bool { _, ok := in.(*Record); return ok }

func (t *taggedOption) decode(fb *Fiber, in Value) (Value, []issue) {
	obj, ok := in.(*Record)
	if !ok {
		return nil, expected("an object with `_tag` None or Some", in)
	}
	switch recordField(obj, "_tag") {
	case "None":
		if _, issues := object(fb, obj, []string{"_tag"}, []*Schema{nil}); issues != nil {
			return nil, issues
		}
		return None, nil
	case "Some":
		fields, issues := object(fb, obj, []string{"_tag", "value"}, []*Schema{nil, nil})
		if issues != nil {
			return nil, issues
		}
		v, issues := t.item.decode(fb, fields[1])
		if issues != nil {
			return nil, under("value", issues)
		}
		return Some(v), nil
	}
	return nil, under("_tag", fail("expected one of None, Some"))
}

func (t *taggedOption) encode(fb *Fiber, v Value) (Value, []issue) {
	if !isSome(v) {
		return Rec("_tag", "None"), nil
	}
	e, issues := t.item.encode(fb, value(v))
	if issues != nil {
		return nil, under("value", issues)
	}
	return Rec("_tag", "Some", "value", e), nil
}

// transformed is a codec decoded further by `to`, and encoded by `back`
// before the codec it is built on.
type transformed struct {
	from codec
	to   func(*Fiber, Value) (Value, []issue)
	back func(*Fiber, Value) Value
	// refusal is why the JSON Schema exporter has no form for it; "" writes the codec's
	refusal string
	// typed is the schema of its values, from `transform_as`; nil takes any value
	typed *Schema
}

func (t *transformed) accepts(in Value) bool { return t.from.accepts(in) }

func (t *transformed) decode(fb *Fiber, in Value) (Value, []issue) {
	v, issues := t.from.decode(fb, in)
	if issues != nil {
		return nil, issues
	}
	return t.to(fb, v)
}

func (t *transformed) encode(fb *Fiber, v Value) (Value, []issue) {
	return t.from.encode(fb, t.back(fb, v))
}

func CodecTransformPure(decode, encode, from Value) Value {
	return &transformed{
		from: codecOf(from),
		to:   func(fb *Fiber, v Value) (Value, []issue) { return Apply(fb, decode, v), nil },
		back: func(fb *Fiber, v Value) Value { return Apply(fb, encode, v) },
	}
}

// issuesOf is the issue a SchemaError of the program gives as a whole.
func issuesOf(err Value) []issue {
	var messages []string
	for _, i := range List(err.(*Data).Fields()[0]) {
		messages = append(messages, i.(*Record).Get("message").(string))
	}
	return fail(strings.Join(messages, "; "))
}

// CodecTransformAs is `transform_as`: `transform` whose values are those of the schema.
func CodecTransformAs(typed, decode, encode, from Value) Value {
	t := CodecTransformPure(decode, encode, from).(*transformed)
	t.typed = sch(typed)
	return t
}

func CodecTransformResult(decode, encode, from Value) Value {
	return &transformed{
		from: codecOf(from),
		to: func(fb *Fiber, v Value) (Value, []issue) {
			r := Apply(fb, decode, v)
			if !isOk(r) {
				return nil, issuesOf(value(r))
			}
			return value(r), nil
		},
		back:    func(fb *Fiber, v Value) Value { return Apply(fb, encode, v) },
		refusal: "a fallible transformation has no JSON Schema form",
	}
}

// CodecCheckWhole is `Codec.check`: a whole-value check after decoding.
func CodecCheckWhole(check, message, from Value) Value {
	return &transformed{
		from: codecOf(from),
		to: func(fb *Fiber, v Value) (Value, []issue) {
			if !Bool(Apply(fb, check, v)) {
				return nil, fail(message.(string))
			}
			return v, nil
		},
		back:    func(_ *Fiber, v Value) Value { return v },
		refusal: `the check "` + message.(string) + `" has no JSON Schema form`,
	}
}
