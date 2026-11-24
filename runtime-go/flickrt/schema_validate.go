package flickrt

import (
	"math"
	"math/big"
	"strconv"
)

// `Schema.validate`: whether an input is a value of the type, by the type
// side of its schema, with the JS runtime's names of what each kind
// expects.

// customSchema is the schema of a type of the runtime: `valid` checks a
// value of it and `json` is its canonical JSON codec.
func customSchema(json codec, valid func(*Fiber, Value) (Value, []issue)) Value {
	s := &Schema{kind: kindCustom, valid: valid}
	s.wireOnce.Do(func() { s.wire = json })
	return s
}

// needing is the custom schema, of a type Schema.arbitrary names `what`.
func needing(what string, s Value) Value {
	sch(s).needs = what
	return s
}

func SchemaValidate(fb *Fiber, s, in Value) Value {
	v, issues := sch(s).validate(fb, in)
	if issues != nil {
		return Err(schemaErrorOf(issues))
	}
	return Ok(v)
}

func (s *Schema) validate(fb *Fiber, in Value) (Value, []issue) {
	s = s.resolved()
	switch s.kind {
	case kindNumber:
		if _, ok := in.(float64); !ok {
			return nil, expected("a number", in)
		}
	case kindInt:
		x, ok := number(in)
		if !ok {
			return nil, expected("a number", in)
		}
		if x != math.Trunc(x) || x < math.MinInt32 || x > math.MaxInt32 || x == 0 && math.Signbit(x) {
			return nil, fail("expected an Int, an integer from -2147483648 to 2147483647")
		}
		return int32(x), nil
	case kindString:
		if _, ok := in.(string); !ok {
			return nil, expected("a string", in)
		}
	case kindBool:
		if _, ok := in.(bool); !ok {
			return nil, expected("a boolean", in)
		}
	case kindBigint:
		if _, ok := in.(*big.Int); !ok {
			return nil, expected("a BigInt", in)
		}
	case kindUnit:
		if in != Unit {
			return nil, expected("()", in)
		}
	case kindNever:
		return nil, expected("nothing (Never)", in)
	case kindRedacted:
		r, ok := in.(*Redacted)
		if !ok {
			return nil, expected("Redacted", in)
		}
		if _, issues := s.item.validate(fb, r.value); issues != nil {
			return nil, issues
		}
	case kindList:
		xs, ok := in.([]Value)
		if !ok {
			return nil, expected("a list", in)
		}
		return validateAll(fb, xs, func(int) *Schema { return s.item })
	case kindTuple:
		xs, ok := in.([]Value)
		if !ok {
			return nil, expected("a tuple of "+strconv.Itoa(len(s.items)), in)
		}
		if len(xs) < len(s.items) {
			return nil, under(len(xs), fail("a missing key"))
		}
		if len(xs) > len(s.items) {
			return nil, under(len(s.items), fail("an unknown key"))
		}
		return validateAll(fb, xs, func(i int) *Schema { return s.items[i] })
	case kindOption:
		if IsNone(in) {
			return in, nil
		}
		if !IsSome(in) {
			return nil, expected("Option", in)
		}
		v, issues := s.item.validate(fb, UnwrapSome(in))
		if issues != nil {
			return nil, issues
		}
		return Some(v), nil
	case kindResult:
		d, ok := in.(*Data)
		item := s.items[0]
		if ok && d.Tag == "Success" {
			item = s.items[1]
		}
		if !ok || len(d.Fields()) != 1 || (d.Tag != "Success" && d.Tag != "Failure") {
			return nil, expected("Result", in)
		}
		v, issues := item.validate(fb, d.Fields()[0])
		if issues != nil {
			return nil, issues
		}
		return &Data{Tag: d.Tag, n: 1, in: [2]Value{v}}, nil
	case kindRecord:
		r, ok := in.(RecordView)
		if !ok {
			return nil, expected("a record", in)
		}
		fields := make([]Value, len(s.labels))
		for i, l := range s.labels {
			v, present := r.Lookup(l)
			if !present {
				return nil, under(l, fail("a missing key"))
			}
			x, issues := s.items[i].validate(fb, v)
			if issues != nil {
				return nil, under(l, issues)
			}
			fields[i] = x
		}
		return shaped(s.keys, s.pos, fields), nil
	case kindAdt:
		d, ok := in.(Constructed)
		if ok {
			dFields := d.Fields()
			for at, variant := range s.variants {
				if variant.Tag != d.TagName() || len(dFields) != len(variant.Fields) {
					continue
				}
				fields := make([]Value, len(dFields))
				for i, f := range dFields {
					v, issues := s.fields[at][i+1].validate(fb, f)
					if issues != nil {
						return nil, under("_"+strconv.Itoa(i), issues)
					}
					fields[i] = v
				}
				return Construct(d.TagName(), fields...), nil
			}
		}
		return nil, expected("a "+s.name, in)
	case kindRefined:
		v, issues := s.item.validate(fb, in)
		if issues == nil {
			issues = s.refine(fb, v)
		}
		return v, issues
	case kindCustom:
		return s.valid(fb, in)
	}
	return in, nil
}

// validateAll checks each element against its schema.
func validateAll(fb *Fiber, xs []Value, schema func(int) *Schema) (Value, []issue) {
	out := make([]Value, len(xs))
	for i, x := range xs {
		v, issues := schema(i).validate(fb, x)
		if issues != nil {
			return nil, under(i, issues)
		}
		out[i] = v
	}
	return out, nil
}
