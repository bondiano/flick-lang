package flickrt

// The codecs of `codec` blocks: a record of labelled fields, each under
// its wire name, as Effect's Struct with renamed keys reads and writes it.
// An object with a key no field names is refused unless the block ignores
// extra fields.

// A fieldSpec is `field wire codec` before the block labels it.
type fieldSpec struct {
	wire     string
	codec    codec
	optional bool // a missing key is None, None is not written
}

func DslField(wire, c Value) Value         { return &fieldSpec{wire.(string), codecOf(c), false} }
func DslOptionalField(wire, c Value) Value { return &fieldSpec{wire.(string), codecOf(c), true} }

var CodecString Value = sch(SchemaString).jsonOf()

type blockField struct {
	label string
	*fieldSpec
}

type block struct {
	name        string
	fields      []blockField
	ignoreExtra bool
}

// CodecBlock is the codec of the block `name` of labels and fields, in
// turn; two fields of one wire name are a defect.
func CodecBlock(name string, entries []Value) Value {
	b := &block{name: name}
	for i := 0; i < len(entries); i += 2 {
		f := blockField{entries[i].(string), entries[i+1].(*fieldSpec)}
		for _, g := range b.fields {
			if g.wire == f.wire {
				panic(Defect{"two fields of the codec of " + name + ` use the wire name "` + f.wire + `"`})
			}
		}
		b.fields = append(b.fields, f)
	}
	return b
}

// CodecIgnoreExtra is the block dropping the keys it does not name; any
// other codec as it is.
func CodecIgnoreExtra(c Value) Value {
	if b, ok := c.(*block); ok {
		return &block{b.name, b.fields, true}
	}
	return c
}

func (b *block) accepts(in Value) bool { _, ok := in.(*Record); return ok }

func (b *block) decode(fb *Fiber, in Value) (Value, []issue) {
	obj, ok := in.(*Record)
	if !ok {
		return nil, expected("an object", in)
	}
	if !b.ignoreExtra {
		for _, k := range obj.keys {
			known := false
			for _, f := range b.fields {
				known = known || f.wire == k
			}
			if !known {
				return nil, under(k, fail("an unknown key"))
			}
		}
	}
	out := make([]Value, 0, 2*len(b.fields))
	for _, f := range b.fields {
		v := recordField(obj, f.wire)
		if v == nil {
			if !f.optional {
				return nil, under(f.wire, fail("a missing key"))
			}
			out = append(out, f.label, None)
			continue
		}
		x, issues := f.codec.decode(fb, v)
		if issues != nil {
			return nil, under(f.wire, issues)
		}
		if f.optional {
			x = Some(x)
		}
		out = append(out, f.label, x)
	}
	return Rec(out...), nil
}

func (b *block) encode(fb *Fiber, v Value) (Value, []issue) {
	r := record(v)
	out := newRecord(len(b.fields))
	for _, f := range b.fields {
		x := r.Get(f.label)
		if f.optional {
			o, ok := some(x)
			if !ok {
				continue
			}
			x = o
		}
		e, issues := f.codec.encode(fb, x)
		if issues != nil {
			return nil, under(f.wire, issues)
		}
		out.put(f.wire, e)
	}
	return out, nil
}
