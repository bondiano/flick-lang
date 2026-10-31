package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"sync"
)

// A Schema is the witness of a type: what its values are, and how its
// canonical JSON form reads and writes them, as the JS runtime's schemas
// over Effect Schema do. Decoding stops at the first issue, except that an
// untagged union reports the first issue of each case.
type Schema struct {
	kind     schemaKind
	item     *Schema     // list, option, redacted
	items    []*Schema   // tuple; result: error, then value
	labels   []string    // record, in label order
	keys     []string    // record, in key order
	pos      []int       // record: where each label's value is in key order
	variants []Variant   // adt
	fields   [][]*Schema // adt: nil for the tag, then each field's
	wireKeys [][]string  // adt: `_tag` then each field's key
	name     string      // adt, lazy, refined
	check    Value       // refined: its check, a function to Bool
	known    string      // refined: the check JSON Schema can state, or ""
	get      func() Value
	valid    func(*Fiber, Value) (Value, []issue) // custom: the check of a value of the type
	// needs is what Schema.arbitrary of a custom or opaque type says needs a generator
	needs     string
	generator gen // Schema.with_generator's, or nil
	once      sync.Once
	target    *Schema // a lazy schema, resolved
	wireOnce  sync.Once
	wire      codec
}

type schemaKind int

const (
	kindNumber schemaKind = iota
	kindInt
	kindString
	kindBool
	kindBigint
	kindUnit
	kindNever
	kindUnknown
	kindList
	kindOption
	kindResult
	kindRedacted
	kindTuple
	kindRecord
	kindAdt
	kindLazy
	kindRefined
	kindCustom // a type of the runtime: its check and its JSON codec
)

// Variant is a constructor of a variant type: its qualified tag, its name
// on the wire and the schemas of its fields.
type Variant struct {
	Tag    string
	Name   string
	Fields []Value
}

func sch(v Value) *Schema { return v.(*Schema) }

var (
	SchemaNumber  Value = &Schema{kind: kindNumber}
	SchemaInt     Value = &Schema{kind: kindInt}
	SchemaString  Value = &Schema{kind: kindString}
	SchemaBool    Value = &Schema{kind: kindBool}
	SchemaBigint  Value = &Schema{kind: kindBigint}
	SchemaUnit    Value = &Schema{kind: kindUnit}
	SchemaNever   Value = &Schema{kind: kindNever}
	SchemaUnknown Value = &Schema{kind: kindUnknown}
)

func SchemaList(item Value) Value     { return &Schema{kind: kindList, item: sch(item)} }
func SchemaOption(item Value) Value   { return &Schema{kind: kindOption, item: sch(item)} }
func SchemaRedacted(item Value) Value { return &Schema{kind: kindRedacted, item: sch(item)} }

func SchemaResult(err, value Value) Value {
	return &Schema{kind: kindResult, items: []*Schema{sch(err), sch(value)}}
}

func SchemaTuple(items ...Value) Value {
	out := make([]*Schema, len(items))
	for i, s := range items {
		out[i] = sch(s)
	}
	return &Schema{kind: kindTuple, items: out}
}

// SchemaRecord is a record of labels and schemas, alternating, in label
// order.
func SchemaRecord(fields ...Value) Value {
	s := &Schema{kind: kindRecord}
	for i := 0; i < len(fields); i += 2 {
		s.labels = append(s.labels, fields[i].(string))
		s.items = append(s.items, sch(fields[i+1]))
	}
	s.keys, s.pos = shapeOf(s.labels)
	return s
}

func SchemaAdt(name string, variants []Variant) Value {
	s := &Schema{kind: kindAdt, name: name, variants: variants}
	for _, v := range variants {
		keys, schemas := []string{"_tag"}, []*Schema{nil}
		for i, f := range v.Fields {
			keys = append(keys, "_"+strconv.Itoa(i))
			schemas = append(schemas, sch(f))
		}
		s.wireKeys = append(s.wireKeys, keys)
		s.fields = append(s.fields, schemas)
	}
	return s
}

// SchemaLazy is a schema resolved on first use: recursive types.
// SchemaRefined is the witness of a refined type: its base's, with the
// refined type's check on what it decodes and encodes.
func SchemaRefined(name string, base, check Value, known string) Value {
	return &Schema{kind: kindRefined, item: sch(base), name: name, check: check, known: known}
}

// refine checks a value of the refined type.
func (s *Schema) refine(fb *Fiber, v Value) []issue {
	if !Apply(fb, s.check, v).(bool) {
		return fail("not a valid " + s.name)
	}
	return nil
}

// RefinedParse is a refined type's `parse`: its check as a Result.
func RefinedParse(name string, check Value) Value {
	return Fn(func(fb *Fiber, v Value) Value {
		if !Apply(fb, check, v).(bool) {
			return Err(schemaErrorOf(fail("not a valid " + name)))
		}
		return Ok(v)
	})
}

// RefinedValue is a refined type's `value`: the value of its base type.
var RefinedValue Value = Fn(func(_ *Fiber, v Value) Value { return v })

func SchemaLazy(name string, get func() Value) Value {
	return &Schema{kind: kindLazy, name: name, get: get}
}

func (s *Schema) resolved() *Schema {
	if s.kind != kindLazy {
		return s
	}
	s.once.Do(func() { s.target = sch(s.get()) })
	return s.target.resolved()
}

// A codec reads values from a wire form and writes them to it.
type codec interface {
	decode(fb *Fiber, in Value) (Value, []issue)
	encode(fb *Fiber, v Value) (Value, []issue)
	// accepts is whether an untagged union tries this case for `in`.
	accepts(in Value) bool
}

// jsonOf is the codec of a schema's canonical JSON form.
func (s *Schema) jsonOf() codec {
	s.wireOnce.Do(func() { s.wire = &wire{s} })
	return s.wire
}

type wire struct{ s *Schema }

func fail(message string) []issue { return []issue{{"", message}} }

func expected(what string, in Value) []issue {
	return fail("expected " + what + ", got " + describeInput(in))
}

var digitsPattern = regexp.MustCompile(`^-?\d+$`)

const redactedRefusal = "a Redacted value has no JSON form without an explicit policy"

func number(in Value) (float64, bool) {
	switch x := in.(type) {
	case float64:
		return x, true
	case int32:
		return float64(x), true
	}
	return 0, false
}

func (w *wire) accepts(in Value) bool {
	s := w.s.resolved()
	switch s.kind {
	case kindRefined:
		return s.item.jsonOf().accepts(in)
	case kindNumber, kindInt:
		_, ok := number(in)
		return ok
	case kindString, kindBigint:
		_, ok := in.(string)
		return ok
	case kindBool:
		_, ok := in.(bool)
		return ok
	case kindUnit:
		return in == JsonNull
	case kindNever:
		return false
	case kindList, kindTuple:
		_, ok := in.([]Value)
		return ok
	case kindRecord, kindAdt, kindResult:
		_, ok := in.(*Record)
		return ok
	case kindOption:
		return in == JsonNull || s.item.jsonOf().accepts(in)
	case kindCustom:
		return s.jsonOf().accepts(in)
	}
	return true
}

func (w *wire) decode(fb *Fiber, in Value) (Value, []issue) {
	s := w.s.resolved()
	switch s.kind {
	case kindRefined:
		v, issues := s.item.jsonOf().decode(fb, in)
		if issues == nil {
			issues = s.refine(fb, v)
		}
		return v, issues
	case kindNumber:
		x, ok := number(in)
		if !ok || math.IsNaN(x) || math.IsInf(x, 0) {
			return nil, expected("a finite number", in)
		}
		return x, nil
	case kindInt:
		x, ok := number(in)
		if !ok {
			return nil, expected("an Int", in)
		}
		if x != math.Trunc(x) || x < math.MinInt32 || x > math.MaxInt32 || x == 0 && math.Signbit(x) {
			return nil, fail("expected an Int, an integer from -2147483648 to 2147483647")
		}
		return int32(x), nil
	case kindString:
		if _, ok := in.(string); !ok {
			return nil, expected("a string", in)
		}
		return in, nil
	case kindBool:
		if _, ok := in.(bool); !ok {
			return nil, expected("a boolean", in)
		}
		return in, nil
	case kindBigint:
		text, ok := in.(string)
		if !ok {
			return nil, expected("a string of digits", in)
		}
		if !digitsPattern.MatchString(text) {
			return nil, fail("expected a string of digits")
		}
		n, _ := new(big.Int).SetString(text, 10)
		return n, nil
	case kindUnit:
		if in != JsonNull {
			return nil, expected("null", in)
		}
		return Unit, nil
	case kindNever:
		return nil, expected("nothing (Never)", in)
	case kindUnknown:
		return in, nil
	case kindRedacted:
		return nil, fail(redactedRefusal)
	case kindCustom:
		return s.jsonOf().decode(fb, in)
	case kindList:
		xs, ok := in.([]Value)
		if !ok {
			return nil, expected("an array", in)
		}
		item := s.item.jsonOf()
		out := make([]Value, len(xs))
		for i, x := range xs {
			v, issues := item.decode(fb, x)
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = v
		}
		return out, nil
	case kindTuple:
		xs, ok := in.([]Value)
		if !ok {
			return nil, expected("an array", in)
		}
		out := make([]Value, len(s.items))
		for i, item := range s.items {
			if i >= len(xs) {
				return nil, under(i, fail("a missing key"))
			}
			v, issues := item.jsonOf().decode(fb, xs[i])
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = v
		}
		if len(xs) > len(s.items) {
			return nil, under(len(s.items), fail("an unknown key"))
		}
		return out, nil
	case kindOption:
		if in == JsonNull {
			return None, nil
		}
		item := s.item.jsonOf()
		if !item.accepts(in) {
			return nil, expected("Union", in)
		}
		v, issues := item.decode(fb, in)
		if issues != nil {
			return nil, issues
		}
		return Some(v), nil
	case kindResult:
		obj, ok := in.(*Record)
		if !ok {
			return nil, expected("an object with `ok` or `err`", in)
		}
		okField, okIssues := object(fb, obj, []string{"ok"}, []*Schema{s.items[1]})
		if okIssues == nil {
			return Ok(okField[0]), nil
		}
		err, errIssues := object(fb, obj, []string{"err"}, []*Schema{s.items[0]})
		if errIssues == nil {
			return Err(err[0]), nil
		}
		return nil, append(okIssues, errIssues...)
	case kindRecord:
		obj, ok := in.(*Record)
		if !ok {
			return nil, expected("an object", in)
		}
		fields, issues := object(fb, obj, s.labels, s.items)
		if issues != nil {
			return nil, issues
		}
		return shaped(s.keys, s.pos, fields), nil
	case kindAdt:
		obj, ok := in.(*Record)
		if !ok {
			return nil, expected("an object with a `_tag`", in)
		}
		name, _ := recordField(obj, "_tag").(string)
		for at, v := range s.variants {
			if v.Name != name {
				continue
			}
			fields, issues := object(fb, obj, s.wireKeys[at], s.fields[at])
			if issues != nil {
				return nil, issues
			}
			return NewData(v.Tag, fields[1:]...), nil
		}
		names := ""
		for i, v := range s.variants {
			if i > 0 {
				names += ", "
			}
			names += v.Name
		}
		return nil, under("_tag", fail("expected one of "+names))
	}
	panic(Defect{"a schema of no kind"})
}

func recordField(r *Record, k string) Value {
	if i := r.index(k); i >= 0 {
		return r.vals[i]
	}
	return nil
}

// object decodes the fields `keys` of a JSON object: a key it does not
// know comes first, then a missing one, then each field in order. A nil
// schema takes the value as it is.
func object(fb *Fiber, obj *Record, keys []string, schemas []*Schema) ([]Value, []issue) {
	for _, k := range obj.keys {
		known := false
		for _, want := range keys {
			known = known || k == want
		}
		if !known {
			return nil, under(k, fail("an unknown key"))
		}
	}
	out := make([]Value, len(keys))
	for i, k := range keys {
		v := recordField(obj, k)
		if v == nil {
			return nil, under(k, fail("a missing key"))
		}
		if schemas[i] == nil {
			out[i] = v
			continue
		}
		x, issues := schemas[i].jsonOf().decode(fb, v)
		if issues != nil {
			return nil, under(k, issues)
		}
		out[i] = x
	}
	return out, nil
}

func (w *wire) encode(fb *Fiber, v Value) (Value, []issue) {
	s := w.s.resolved()
	switch s.kind {
	case kindRefined:
		if issues := s.refine(fb, v); issues != nil {
			return nil, issues
		}
		return s.item.jsonOf().encode(fb, v)
	case kindNumber:
		x := v.(float64)
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return nil, fail(ShowNumber(x) + " has no JSON form")
		}
		return x, nil
	case kindInt:
		return float64(v.(int32)), nil
	case kindBigint:
		return v.(*big.Int).String(), nil
	case kindUnit:
		return JsonNull, nil
	case kindNever:
		return nil, expected("nothing (Never)", v)
	case kindRedacted:
		return nil, fail(redactedRefusal)
	case kindCustom:
		return s.jsonOf().encode(fb, v)
	case kindString, kindBool, kindUnknown:
		return v, nil
	case kindList, kindTuple:
		xs := List(v)
		out := make([]Value, len(xs))
		for i, x := range xs {
			item := s.item
			if s.kind == kindTuple {
				item = s.items[i]
			}
			e, issues := item.jsonOf().encode(fb, x)
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = e
		}
		return out, nil
	case kindOption:
		o := v.(*Data)
		if o.Tag != "Some" {
			return JsonNull, nil
		}
		return s.item.jsonOf().encode(fb, o.Fields[0])
	case kindResult:
		key, part := "err", s.items[0]
		if isOk(v) {
			key, part = "ok", s.items[1]
		}
		e, issues := part.jsonOf().encode(fb, value(v))
		if issues != nil {
			return nil, under(key, issues)
		}
		return Rec(key, e), nil
	case kindRecord:
		r := v.(*Record)
		out := make([]Value, len(s.labels))
		for i, l := range s.labels {
			e, issues := s.items[i].jsonOf().encode(fb, r.Get(l))
			if issues != nil {
				return nil, under(l, issues)
			}
			out[i] = e
		}
		return shaped(s.keys, s.pos, out), nil
	case kindAdt:
		d := v.(*Data)
		for at, variant := range s.variants {
			if variant.Tag != d.Tag {
				continue
			}
			keys := s.wireKeys[at]
			out := make([]Value, len(keys))
			out[0] = variant.Name
			for i, f := range s.fields[at][1:] {
				e, issues := f.jsonOf().encode(fb, d.Fields[i])
				if issues != nil {
					return nil, under(keys[i+1], issues)
				}
				out[i+1] = e
			}
			return &Record{keys, out}, nil
		}
	}
	panic(Defect{"a value its schema does not describe"})
}

// emailPattern is the JS target's: one `@` with text on both sides and a
// dot in the domain.
var emailPattern = func() *regexp.Regexp {
	// JS's \s, which reaches past ASCII
	other := `[^\t\n\v\f\r \x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}@]+`
	return regexp.MustCompile(`^` + other + `@` + other + `\.` + other + `$`)
}()

func CheckEmail(s Value) Value    { return emailPattern.MatchString(s.(string)) }
func CheckNonEmpty(s Value) Value { return s.(string) != "" }
func CheckPositive(n Value) Value { return n.(float64) > 0 }
