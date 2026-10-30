package flickrt

import (
	"math"
	"strings"
)

// Codecs of the `Codec` module: the canonical JSON codec of a schema, and
// codecs built out of codecs.

func SchemaJsonCodec(s Value) Value { return sch(s).jsonOf() }

// JsonCodec is `Schema.json`: a JSON value as itself.
var JsonCodec Value = sch(SchemaUnknown).jsonOf()

var CodecBool Value = sch(SchemaBool).jsonOf()

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

func (n losslessNumber) decode(in Value) (Value, []issue) {
	if !n.accepts(in) {
		return nil, expected("a number", in)
	}
	if s, ok := in.(string); ok {
		return losslessNames[s], nil
	}
	return in, nil
}

func (losslessNumber) encode(v Value) (Value, []issue) {
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

func CodecDecodeResult(c, in Value) Value {
	v, issues := codecOf(c).decode(in)
	if issues != nil {
		return Err(schemaErrorOf(issues))
	}
	return Ok(v)
}

func CodecEncodeResult(c, v Value) Value {
	e, issues := codecOf(c).encode(v)
	if issues != nil {
		return Err(schemaErrorOf(issues))
	}
	return Ok(e)
}

// CodecDecode and CodecEncode fail with the SchemaError.
func CodecDecode(_ *Fiber, c, in Value) Value { return FromResult(CodecDecodeResult(c, in)) }
func CodecEncode(_ *Fiber, c, v Value) Value  { return FromResult(CodecEncodeResult(c, v)) }

// listCodec is `Codec.list`: a JSON array of the item's wire form.
type listCodec struct{ item codec }

func CodecList(item Value) Value { return &listCodec{codecOf(item)} }

func (l *listCodec) accepts(in Value) bool { _, ok := in.([]Value); return ok }

func (l *listCodec) decode(in Value) (Value, []issue) {
	xs, ok := in.([]Value)
	if !ok {
		return nil, expected("an array", in)
	}
	out := make([]Value, len(xs))
	for i, x := range xs {
		v, issues := l.item.decode(x)
		if issues != nil {
			return nil, under(i, issues)
		}
		out[i] = v
	}
	return out, nil
}

func (l *listCodec) encode(v Value) (Value, []issue) {
	xs := List(v)
	out := make([]Value, len(xs))
	for i, x := range xs {
		e, issues := l.item.encode(x)
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

func (t *taggedOption) decode(in Value) (Value, []issue) {
	obj, ok := in.(*Record)
	if !ok {
		return nil, expected("an object with `_tag` None or Some", in)
	}
	switch recordField(obj, "_tag") {
	case "None":
		if _, issues := object(obj, []string{"_tag"}, []*Schema{nil}); issues != nil {
			return nil, issues
		}
		return None, nil
	case "Some":
		fields, issues := object(obj, []string{"_tag", "value"}, []*Schema{nil, nil})
		if issues != nil {
			return nil, issues
		}
		v, issues := t.item.decode(fields[1])
		if issues != nil {
			return nil, under("value", issues)
		}
		return Some(v), nil
	}
	return nil, under("_tag", fail("expected one of None, Some"))
}

func (t *taggedOption) encode(v Value) (Value, []issue) {
	if !isSome(v) {
		return Rec("_tag", "None"), nil
	}
	e, issues := t.item.encode(value(v))
	if issues != nil {
		return nil, under("value", issues)
	}
	return Rec("_tag", "Some", "value", e), nil
}

// transformed is a codec decoded further by `to`, and encoded by `back`
// before the codec it is built on.
type transformed struct {
	from codec
	to   func(Value) (Value, []issue)
	back func(Value) Value
	// refusal is why the JSON Schema exporter has no form for it; "" writes the codec's
	refusal string
}

func (t *transformed) accepts(in Value) bool { return t.from.accepts(in) }

func (t *transformed) decode(in Value) (Value, []issue) {
	v, issues := t.from.decode(in)
	if issues != nil {
		return nil, issues
	}
	return t.to(v)
}

func (t *transformed) encode(v Value) (Value, []issue) { return t.from.encode(t.back(v)) }

func CodecTransformPure(decode, encode, from Value) Value {
	return &transformed{
		from: codecOf(from),
		to:   func(v Value) (Value, []issue) { return Apply(decode, v), nil },
		back: func(v Value) Value { return Apply(encode, v) },
	}
}

// issuesOf is the issue a SchemaError of the program gives as a whole.
func issuesOf(err Value) []issue {
	var messages []string
	for _, i := range List(err.(*Data).Fields[0]) {
		messages = append(messages, i.(*Record).Get("message").(string))
	}
	return fail(strings.Join(messages, "; "))
}

func CodecTransformResult(decode, encode, from Value) Value {
	return &transformed{
		from: codecOf(from),
		to: func(v Value) (Value, []issue) {
			r := Apply(decode, v)
			if !isOk(r) {
				return nil, issuesOf(value(r))
			}
			return value(r), nil
		},
		back:    func(v Value) Value { return Apply(encode, v) },
		refusal: "a fallible transformation has no JSON Schema form",
	}
}

// CodecCheckWhole is `Codec.check`: a whole-value check after decoding.
func CodecCheckWhole(check, message, from Value) Value {
	return &transformed{
		from: codecOf(from),
		to: func(v Value) (Value, []issue) {
			if !Bool(Apply(check, v)) {
				return nil, fail(message.(string))
			}
			return v, nil
		},
		back:    func(v Value) Value { return v },
		refusal: `the check "` + message.(string) + `" has no JSON Schema form`,
	}
}
