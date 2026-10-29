// Package flickrt is the runtime of Flick programs compiled to Go.
//
// The value ABI mirrors the JS target's: Int is int32, Float float64,
// BigInt *big.Int, String a WTF-8 string, Bool bool, Unit the empty struct,
// a tuple or a list a []Value, a record a Record, a value of a variant type
// a *Data with a qualified tag, a function of one argument an Fn and a
// function of class dictionaries a Poly.
package flickrt

import (
	"fmt"
	"strings"
)

// Value is any Flick value.
type Value = any

// Fn is a function of one stage.
type Fn = func(Value) Value

// Poly is a value that takes the dictionaries of its constraints first.
type Poly = func(...Value) Value

// UnitType is the type of Unit.
type UnitType struct{}

// Unit is the one value of `Unit`.
var Unit Value = UnitType{}

// Data is a value of a variant type: its constructor's tag and fields.
type Data struct {
	Tag    string
	Fields []Value
}

// Record is a record value; it is never mutated after it is built.
type Record map[string]Value

// Option and Result have the tags of the JS runtime's native values.
var None Value = &Data{Tag: "None"}

func Some(v Value) Value { return &Data{Tag: "Some", Fields: []Value{v}} }
func Ok(v Value) Value   { return &Data{Tag: "Success", Fields: []Value{v}} }
func Err(v Value) Value  { return &Data{Tag: "Failure", Fields: []Value{v}} }

// Ordering values, tagged as the compiler tags Prelude's constructors.
var (
	LT Value = &Data{Tag: Tag("Prelude.Ordering.LT")}
	EQ Value = &Data{Tag: Tag("Prelude.Ordering.EQ")}
	GT Value = &Data{Tag: Tag("Prelude.Ordering.GT")}
)

// Tag is the tag of the constructor with the qualified id `id`: its name
// and the 48 high bits of the FNV-1a 64 hash of the id.
func Tag(id string) string {
	h := uint64(0xcbf29ce484222325)
	for i := 0; i < len(id); i++ {
		h = (h ^ uint64(id[i])) * 0x100000001b3
	}
	return fmt.Sprintf("%s#%012x", id[strings.LastIndexByte(id, '.')+1:], h>>16)
}

// ConstructorName is the name in a tag: `Circle` of `Circle#…`.
func ConstructorName(tag string) string {
	if i := strings.IndexByte(tag, '#'); i >= 0 {
		return tag[:i]
	}
	return tag[strings.LastIndexByte(tag, '.')+1:]
}

// NewData builds a value of a constructor with its fields.
func NewData(tag string, fields ...Value) Value {
	return &Data{Tag: tag, Fields: fields}
}

// IsTag tells whether `v` is a value of the constructor tagged `tag`.
func IsTag(v Value, tag string) bool { return v.(*Data).Tag == tag }

// TagOf is the tag of a variant value.
func TagOf(v Value) Value { return v.(*Data).Tag }

// Get is the field `i` of a variant value.
func Get(v Value, i int) Value { return v.(*Data).Fields[i] }

// Apply calls a function of one stage.
func Apply(f Value, a Value) Value { return f.(Fn)(a) }

// Inst gives a constrained value its dictionaries.
func Inst(f Value, dicts ...Value) Value { return f.(Poly)(dicts...) }

// Bool is a Bool as a Go condition.
func Bool(v Value) bool { return v.(bool) }

// Not negates a Bool.
func Not(v Value) bool { return !v.(bool) }

// Field is the field `label` of a record, or the tag of a variant value.
func Field(v Value, label string) Value {
	if label == "_tag" {
		if d, ok := v.(*Data); ok {
			return d.Tag
		}
	}
	return v.(Record)[label]
}

// Update is a copy of a record with some fields replaced.
func Update(base Value, fields Record) Value {
	b := base.(Record)
	out := make(Record, len(b)+len(fields))
	for k, v := range b {
		out[k] = v
	}
	for k, v := range fields {
		out[k] = v
	}
	return out
}

// Len is the length of a list or a tuple.
func Len(v Value) int { return len(v.([]Value)) }

// At is the element `i` of a list or a tuple.
func At(v Value, i int) Value { return v.([]Value)[i] }

// Rest is a list without its first `n` elements; it shares their storage.
func Rest(v Value, n int) Value { return v.([]Value)[n:] }

// List is the elements of a list.
func List(v Value) []Value { return v.([]Value) }

// ListConcat is `xs ++ ys` on lists: a new list, never sharing a tail that
// a later append could overwrite.
func ListConcat(a, b Value) Value {
	xs, ys := a.([]Value), b.([]Value)
	out := make([]Value, 0, len(xs)+len(ys))
	return append(append(out, xs...), ys...)
}

// Curry2 is a function of two arguments taken one at a time.
func Curry2(f func(Value, Value) Value) Fn {
	return func(a Value) Value { return Fn(func(b Value) Value { return f(a, b) }) }
}

// Curry3 is a function of three arguments taken one at a time.
func Curry3(f func(Value, Value, Value) Value) Fn {
	return func(a Value) Value { return Curry2(func(b, c Value) Value { return f(a, b, c) }) }
}

// Defect stops the program with an error that is not a typed failure.
type Defect struct{ Message string }

func (d Defect) Error() string { return d.Message }

// Die raises a defect.
func Die(message string) Value { panic(Defect{message}) }

// NoMatch is the defect of a `match` no arm matched.
func NoMatch() Value { panic(Defect{"no pattern matched the value"}) }
