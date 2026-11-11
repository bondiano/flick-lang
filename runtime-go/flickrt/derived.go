package flickrt

// The combinators `deriving Schema` writes in Flick, as the JS runtime's
// schema/derived.ts: a constructor of a variant type is given as a value,
// whose tag the runtime reads.

import "strings"

// tagOf is the tag of a constructor given as a value: a nullary one is its
// value, another a function of its fields.
func tagOf(constructor Value) string {
	v := constructor
	for {
		switch f := v.(type) {
		case Fn:
			v = f(nil, nil)
		case Fn2:
			v = f(nil, nil, nil)
		case Fn3:
			v = f(nil, nil, nil, nil)
		default:
			return v.(*Data).Tag
		}
	}
}

// SchemaField is `Schema.field label schema`.
func SchemaField(label, schema Value) Value { return []Value{label, schema} }

// SchemaRecordOf is `Schema.unsafe_record fields`, in label order.
func SchemaRecordOf(fields Value) Value {
	var flat []Value
	for _, f := range List(fields) {
		pair := f.([]Value)
		flat = append(flat, pair[0], pair[1])
	}
	return SchemaRecord(flat...)
}

// SchemaItem is `Schema.item schema`: an item of a tuple is its schema.
func SchemaItem(schema Value) Value { return schema }

// SchemaTupleOf is `Schema.unsafe_tuple items`.
func SchemaTupleOf(items Value) Value { return SchemaTuple(List(items)...) }

// schemaCase is a constructor and the schemas of its fields.
type schemaCase struct {
	tag    string
	fields []Value
}

// SchemaCase is `Schema.unsafe_case c fields`.
func SchemaCase(constructor, fields Value) Value {
	return &schemaCase{tag: tagOf(constructor), fields: List(fields)}
}

// SchemaAdtOf is `Schema.unsafe_adt name cases`: a constructor's name on the wire
// is its tag's.
func SchemaAdtOf(name, cases Value) Value {
	var variants []Variant
	for _, c := range List(cases) {
		each := c.(*schemaCase)
		wire, _, _ := strings.Cut(each.tag, "#")
		variants = append(variants, Variant{Tag: each.tag, Name: wire, Fields: each.fields})
	}
	return SchemaAdt(name.(string), variants)
}

// SchemaLazyOf is `Schema.lazy name get`.
func SchemaLazyOf(name, get Value) Value {
	return SchemaLazy(name.(string), func() Value { return Apply(nil, get, nil) })
}

// SchemaRecursive is `Schema.recursive name of`: `of` gets a reference to
// the schema it makes, resolved on use.
func SchemaRecursive(name, of Value) Value {
	var made Value
	self := SchemaLazy(name.(string), func() Value { return made })
	made = Apply(nil, of, self)
	return made
}

// SchemaUnboxed is `Schema.unsafe_unboxed field`: the values are the
// field's.
func SchemaUnboxed(field Value) Value { return field }

// SchemaRefinedOf is `Schema.unsafe_refined name known base parse`.
func SchemaRefinedOf(name, known, base, parse Value) Value {
	check := Fn(func(fb *Fiber, v Value) Value { return Apply(fb, parse, v).(*Data).Tag == "Success" })
	text := ""
	if k := known.(*Data); k.Tag == "Some" {
		text = k.Fields()[0].(string)
	}
	return SchemaRefined(name.(string), base, check, text)
}

// RoutePath is `Route.path c path segments query optional`.
func RoutePath(constructor, path, segments, query, optional Value) Value {
	route := Route{Tag: tagOf(constructor), Path: path.(string), Segments: List(segments), Optional: optional.(bool)}
	if q := query.(*Data); q.Tag == "Some" {
		route.Query = []QueryField{}
		for _, field := range List(q.Fields()[0]) {
			route.Query = append(route.Query, field.(QueryField))
		}
	}
	return route
}

// RouteSegment is `Route.segment schema`: a segment is its field's schema.
func RouteSegment(schema Value) Value { return schema }

// RouteWordsOf is `Route.words words`: each word with its constructor.
func RouteWordsOf(words Value) Value {
	var pairs []string
	for _, w := range List(words) {
		pair := w.([]Value)
		pairs = append(pairs, pair[0].(string), tagOf(pair[1]))
	}
	return RouteWords(pairs)
}

// RouteNestedOf is `Route.nested get`.
func RouteNestedOf(get Value) Value {
	return RouteNested(func() Value { return Apply(nil, get, nil) })
}

// RouteQuery is `Route.query label schema optional`.
func RouteQuery(label, schema, optional Value) Value {
	return QueryField{Label: label.(string), Schema: schema, Optional: optional.(bool)}
}

// RouteCodecOf is `Route.codec schema paths`.
func RouteCodecOf(schema, paths Value) Value {
	var routes []Route
	for _, p := range List(paths) {
		routes = append(routes, p.(Route))
	}
	return RouteCodec(schema, routes)
}
