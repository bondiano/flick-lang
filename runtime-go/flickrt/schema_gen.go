package flickrt

import (
	"math"
	"math/big"
)

// Schema.arbitrary: values of a schema for property tests, drawn by its
// structure as the JS runtime's generator walks the AST of the schema, one
// draw of a size at each node above depth 4. A refinement, or a type
// without a structure, needs an explicit generator.

func needsGenerator(what string) {
	panic(Defect{"RangeError: Schema.arbitrary: " + what + " needs an explicit generator (Schema.with_generator)"})
}

// SchemaWithGenerator is the schema, with `g` for `Schema.arbitrary`.
func SchemaWithGenerator(g, s Value) Value {
	base := sch(s)
	out := &Schema{kind: base.kind, item: base.item, items: base.items, labels: base.labels, keys: base.keys,
		pos: base.pos, variants: base.variants, fields: base.fields, wireKeys: base.wireKeys, name: base.name,
		check: base.check, known: base.known, get: base.get, valid: base.valid, needs: base.needs, generator: g.(gen)}
	if base.kind == kindLazy {
		out.get = func() Value { return base.resolved() }
	}
	out.wireOnce.Do(func() { out.wire = base.jsonOf() })
	return out
}

func SchemaArbitrary(s Value) Value {
	root := sch(s)
	return gen(func(fb *Fiber, r rng) Value { return drawSchema(fb, r, root, 0) })
}

func drawSchema(fb *Fiber, r rng, s *Schema, depth int) Value {
	if s.generator != nil {
		return s.generator(fb, r)
	}
	switch s.kind {
	case kindInt:
		needsGenerator("a check")
	case kindRefined:
		// the checks of nested refinements run from the innermost one
		inner := s
		for inner.item.kind == kindRefined {
			inner = inner.item
		}
		needsGenerator("the check of " + inner.name)
	case kindCustom:
		if e, ok := s.jsonOf().(*enumCodec); ok {
			return drawEnum(r, e, depth)
		}
		// shortcut: a codec's decoded side says it needs a generator, as a declaration does, though JS generates the decoded side of some codecs (strings); walk the codec's type when a program draws from one
		if s.needs == "" {
			needsGenerator("a declared type")
		}
		needsGenerator(s.needs)
	}
	size := 0
	if depth <= 3 {
		size = int(math.Floor(r() * 4))
	}
	next := func(inner *Schema) Value { return drawSchema(fb, r, inner, depth+1) }
	switch s.kind {
	case kindNumber:
		return math.Floor(r()*2000) - 1000
	case kindString:
		b := make([]byte, size)
		for i := range b {
			b[i] = byte(97 + math.Floor(r()*26))
		}
		return string(b)
	case kindBool:
		return r() < 0.5
	case kindBigint:
		return big.NewInt(int64(math.Floor(r()*2000) - 1000))
	case kindUnit:
		return Unit
	case kindList:
		out := make([]Value, size)
		for i := range out {
			out[i] = next(s.item)
		}
		return out
	case kindTuple:
		out := make([]Value, len(s.items))
		for i, item := range s.items {
			out[i] = next(item)
		}
		return out
	case kindRecord:
		// drawn in key order, the order of the properties of a struct
		label := make([]int, len(s.items))
		for i := range label {
			label[i] = i
		}
		for i, at := range s.pos {
			label[at] = i
		}
		out := make([]Value, len(s.items))
		for at, i := range label {
			out[at] = next(s.items[i])
		}
		return &Record{s.keys, out}
	case kindAdt:
		return drawVariant(fb, r, s, depth)
	case kindOption:
		if r() < 0.3 {
			return None
		}
		return Some(next(s.item))
	case kindResult:
		if r() < 0.5 {
			return Err(next(s.items[0]))
		}
		return Ok(next(s.items[1]))
	case kindRedacted:
		return &Redacted{next(s.item)}
	case kindLazy:
		s.resolved()
		return drawSchema(fb, r, s.target, depth)
	case kindNever:
		needsGenerator("Never")
	}
	if s.needs != "" {
		needsGenerator(s.needs)
	}
	needsGenerator("Unknown")
	return nil
}

// drawVariant is a value of a variant type: a union of structs of a `_tag`
// and the fields, a variant without fields past depth 3.
func drawVariant(fb *Fiber, r rng, s *Schema, depth int) Value {
	choices := s.variants
	if depth > 3 {
		var leaves []Variant
		for _, v := range s.variants {
			if len(v.Fields) == 0 {
				leaves = append(leaves, v)
			}
		}
		if len(leaves) > 0 {
			choices = leaves
		}
	}
	v := choices[int(math.Floor(r()*float64(len(choices))))]
	// the struct, then its `_tag`, each a node that draws a size
	for d := depth + 1; d <= depth+2; d++ {
		if d <= 3 {
			r()
		}
	}
	if len(v.Fields) == 0 {
		return Nullary(v.Tag)
	}
	fields := make([]Value, len(v.Fields))
	for i, f := range v.Fields {
		fields[i] = drawSchema(fb, r, sch(f), depth+2)
	}
	return Construct(v.Tag, fields...)
}
