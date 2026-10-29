package flickrt

// Dict is the dictionary of a class instance: its methods, and entries
// computed on first use (superclass dictionaries `$Class` and default
// methods), as the JS target's getters are.
type Dict struct {
	entries map[string]Value
	lazy    map[string]func() Value
}

// NewDict is an empty dictionary to fill with Set and Lazy.
func NewDict() *Dict {
	return &Dict{entries: map[string]Value{}, lazy: map[string]func() Value{}}
}

// Set binds a method.
func (d *Dict) Set(name string, v Value) *Dict {
	d.entries[name] = v
	return d
}

// Lazy binds an entry computed when it is first read.
func (d *Dict) Lazy(name string, f func() Value) *Dict {
	d.lazy[name] = f
	return d
}

func (d *Dict) get(name string) Value {
	if v, ok := d.entries[name]; ok {
		return v
	}
	f, ok := d.lazy[name]
	if !ok {
		panic(Defect{"dictionary has no `" + name + "`"})
	}
	v := f()
	d.entries[name] = v
	return v
}

// Method is the method `name` of a dictionary.
func Method(d Value, name string) Value { return d.(*Dict).get(name) }

// Super is the dictionary of the superclass `class` of a dictionary.
func Super(d Value, class string) Value { return d.(*Dict).get("$" + class) }

// Constructor describes a constructor of a derived type: its tag, name and
// arity, and whether a value is its one field.
type Constructor struct {
	Tag     string
	Name    string
	Arity   int
	Unboxed bool
}

// Derive is a derived instance of class `name`. `fields` gives the
// dictionaries of the fields of all constructors in order, on first use (a
// recursive type needs its own instance); `constructors` is nil for a type
// without variants whose one field is its representation.
func Derive(name string, fields func() []Value, constructors []Constructor, supers map[string]func() Value) Value {
	var cache []Value
	dict := func(k int) Value {
		if cache == nil {
			cache = fields()
		}
		return cache[k]
	}
	variant := func(v Value) (int, int) {
		offset := 0
		for i, c := range constructors {
			if c.Unboxed || v.(*Data).Tag == c.Tag {
				return i, offset
			}
			offset += c.Arity
		}
		panic(Defect{"value of no constructor of its type"})
	}
	args := func(i int, v Value) []Value {
		if constructors[i].Unboxed {
			return []Value{v}
		}
		return v.(*Data).Fields
	}
	d := NewDict()
	for class, f := range supers {
		d.Lazy("$"+class, f)
	}
	switch name {
	case "Eq":
		d.Set("eq", Curry2(func(a, b Value) Value {
			if constructors == nil {
				return Apply(Apply(Method(dict(0), "eq"), a), b)
			}
			i, offset := variant(a)
			if j, _ := variant(b); i != j {
				return false
			}
			left, right := args(i, a), args(i, b)
			for k := range left {
				if !Bool(Apply(Apply(Method(dict(offset+k), "eq"), left[k]), right[k])) {
					return false
				}
			}
			return true
		}))
	case "Ord":
		d.Set("compare", Curry2(func(a, b Value) Value {
			if constructors == nil {
				return Apply(Apply(Method(dict(0), "compare"), a), b)
			}
			i, offset := variant(a)
			if j, _ := variant(b); i != j {
				if i < j {
					return LT
				}
				return GT
			}
			left, right := args(i, a), args(i, b)
			for k := range left {
				o := Apply(Apply(Method(dict(offset+k), "compare"), left[k]), right[k])
				if o.(*Data).Tag != EQ.(*Data).Tag {
					return o
				}
			}
			return EQ
		}))
	case "Hash":
		d.Set("hash", Fn(func(a Value) Value {
			if constructors == nil {
				return Apply(Method(dict(0), "hash"), a)
			}
			i, offset := variant(a)
			h := int32(i)
			for k, x := range args(i, a) {
				h = combine(h, Apply(Method(dict(offset+k), "hash"), x).(int32))
			}
			return h
		}))
	case "Show":
		d.Set("show", Fn(func(a Value) Value {
			if constructors == nil {
				return Apply(Method(dict(0), "show"), a)
			}
			i, offset := variant(a)
			out := constructors[i].Name
			for k, x := range args(i, a) {
				out += " " + atom(Apply(Method(dict(offset+k), "show"), x).(string))
			}
			return out
		}))
	default:
		panic(Defect{"`deriving " + name + "` is not available on the go target"})
	}
	return d
}

// atom wraps a shown argument of a constructor in parentheses when it has
// spaces and does not start with a bracket or a quote.
func atom(text string) string {
	if text == "" || text[0] == '[' || text[0] == '(' || text[0] == '{' || text[0] == '"' {
		return text
	}
	for i := 0; i < len(text); i++ {
		if text[i] == ' ' {
			return "(" + text + ")"
		}
	}
	return text
}

// Structural is the compiler-owned instance of class `name` for a record or
// a tuple: `dicts` are those of its fields, by label order or by position.
func Structural(name string, dicts []Value) Value {
	method := func(i int, m string) Value { return Method(dicts[i], m) }
	eqs := func() Value {
		supers := make([]Value, len(dicts))
		for i, d := range dicts {
			supers[i] = Super(d, "Eq")
		}
		return Structural("Eq", supers)
	}
	d := NewDict()
	switch name {
	case "Eq":
		d.Set("eq", Curry2(func(a, b Value) Value {
			xs, ys := fieldsOf(a), fieldsOf(b)
			for i := range xs {
				if !Bool(Apply(Apply(method(i, "eq"), xs[i]), ys[i])) {
					return false
				}
			}
			return true
		}))
	case "Ord":
		d.Set("compare", Curry2(func(a, b Value) Value {
			xs, ys := fieldsOf(a), fieldsOf(b)
			for i := range xs {
				o := Apply(Apply(method(i, "compare"), xs[i]), ys[i])
				if o.(*Data).Tag != EQ.(*Data).Tag {
					return o
				}
			}
			return EQ
		}))
		d.Lazy("$Eq", eqs)
	case "Hash":
		d.Set("hash", Fn(func(a Value) Value {
			h := int32(11)
			for i, x := range fieldsOf(a) {
				h = combine(h, Apply(method(i, "hash"), x).(int32))
			}
			return h
		}))
		d.Lazy("$Eq", eqs)
	default:
		d.Set("show", Fn(func(a Value) Value {
			xs := fieldsOf(a)
			shown := make([]string, len(xs))
			for i, x := range xs {
				shown[i] = Apply(method(i, "show"), x).(string)
			}
			if r, ok := a.(Record); ok {
				out := "{ "
				for i, l := range sortedKeys(r) {
					if i > 0 {
						out += ", "
					}
					out += l + " = " + shown[i]
				}
				return out + " }"
			}
			return "(" + joinStrings(shown, ", ") + ")"
		}))
	}
	return d
}

// fieldsOf is the fields of a tuple by position, or of a record by label
// in UTF-16 order, as JS sorts its keys.
func fieldsOf(v Value) []Value {
	if r, ok := v.(Record); ok {
		keys := sortedKeys(r)
		out := make([]Value, len(keys))
		for i, k := range keys {
			out[i] = r[k]
		}
		return out
	}
	return v.([]Value)
}
