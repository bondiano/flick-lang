package flickrt

// Constructed is a value of a constructor, whatever its Go type: generated code
// gives a constructor with fields of a program's own type a Go type of its
// own, and every other value of a constructor is a *Data.
type Constructed interface {
	// TagName is the tag of the constructor.
	TagName() string
	// Fields is the fields of the value, in the order of its constructor;
	// the list is the value's own.
	Fields() []Value
}

func (d *Data) TagName() string { return d.Tag }

// tagName and fieldsIn are the tag and the fields of a constructor value,
// read at once from the *Data most of them are.
func tagName(v Value) string {
	if d, ok := v.(*Data); ok {
		return d.Tag
	}
	return v.(Constructed).TagName()
}

func fieldsIn(v Value) []Value {
	if d, ok := v.(*Data); ok {
		return d.Fields()
	}
	return v.(Constructed).Fields()
}

// asData is a value of a constructor as a *Data, for what reads one to print
// it; any other value is itself.
func asData(v Value) Value {
	if _, ok := v.(*Data); ok {
		return v
	}
	if x, ok := v.(Constructed); ok {
		return NewData(x.TagName(), x.Fields()...)
	}
	return v
}

var builders = map[string]func([]Value) Value{}

// Owned tells the runtime how to build a value of the constructor tagged
// `tag`, whose values have a Go type of their own, from its fields.
// Generated code calls it as the program starts.
func Owned(tag string, build func(fields []Value) Value) bool {
	builders[tag] = build
	return true
}

// Construct builds a value of a constructor from its fields in the Go type
// the values of that constructor have. What builds a value of a type it
// does not know, as a decoder does, builds it so.
func Construct(tag string, fields ...Value) Value {
	if build, ok := builders[tag]; ok {
		return build(fields)
	}
	return NewData(tag, fields...)
}
