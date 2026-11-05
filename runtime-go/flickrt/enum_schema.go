package flickrt

import "math"

// The schemas of enums, whose values are their literals: on the wire the
// literals themselves, through the codec of their base type, or the form
// of a variant type, `{"_tag": "Get"}` for each literal's constructor.

type enumCodec struct {
	schema *Schema // of the base type
	base   codec   // its JSON codec
	values []Value
	adt    *Schema // the variant type of the names on the wire, or nil
}

func (e *enumCodec) index(v Value) int {
	for i, x := range e.values {
		if Same(v, x) {
			return i
		}
	}
	return -1
}

func (e *enumCodec) accepts(in Value) bool {
	if e.adt != nil {
		return e.adt.jsonOf().accepts(in)
	}
	return e.base.accepts(in)
}

func (e *enumCodec) decode(fb *Fiber, in Value) (Value, []issue) {
	if e.adt != nil {
		d, issues := e.adt.jsonOf().decode(fb, in)
		if issues != nil {
			return nil, issues
		}
		for i, v := range e.adt.variants {
			if v.Tag == d.(*Data).Tag {
				return e.values[i], nil
			}
		}
	}
	v, issues := e.base.decode(fb, in)
	if issues != nil {
		return nil, issues
	}
	if e.index(v) < 0 {
		return nil, expected("Union", in)
	}
	return v, nil
}

func (e *enumCodec) encode(fb *Fiber, v Value) (Value, []issue) {
	i := e.index(v)
	switch {
	case i < 0:
		return nil, expected("Union", v)
	case e.adt != nil:
		return e.adt.jsonOf().encode(fb, &Data{Tag: e.adt.variants[i].Tag})
	}
	return e.base.encode(fb, v)
}

func (e *enumCodec) jsonSchema(x *schemaExport, path string) Value {
	if e.adt != nil {
		return x.schema(e.adt, path)
	}
	literals := make([]Value, len(e.values))
	for i, v := range e.values {
		literals[i], _ = e.base.encode(nil, v)
	}
	return Rec("enum", literals)
}

func enumSchema(e *enumCodec) Value {
	e.base = e.schema.jsonOf()
	return customSchema(e, func(fb *Fiber, in Value) (Value, []issue) {
		v, issues := e.schema.validate(fb, in)
		if issues != nil {
			return nil, issues
		}
		if e.index(v) < 0 {
			return nil, expected("Union", in)
		}
		return v, nil
	})
}

// drawEnum draws as the JS runtime draws a union of literals: a size at
// the union, the pick, a size at the literal.
func drawEnum(r rng, e *enumCodec, depth int) Value {
	if depth <= 3 {
		r()
	}
	v := e.values[int(math.Floor(r()*float64(len(e.values))))]
	if depth+1 <= 3 {
		r()
	}
	return v
}

// SchemaLiterals is `Schema.unsafe_literals base value values`: an enum's
// values are its literals.
func SchemaLiterals(base, _, values Value) Value {
	return enumSchema(&enumCodec{schema: sch(base), values: List(values)})
}

// SchemaEnumTags is `Schema.unsafe_enum_tags base pairs`: each
// constructor's name on the wire with its value.
func SchemaEnumTags(base, pairs Value) Value {
	e := &enumCodec{schema: sch(base)}
	var variants []Variant
	for _, p := range List(pairs) {
		pair := p.([]Value)
		tag := pair[0].(string)
		variants = append(variants, Variant{Tag: tag, Name: tag})
		e.values = append(e.values, pair[1])
	}
	e.adt = sch(SchemaAdt("", variants))
	return enumSchema(e)
}
