package flickrt

// The explicit wire policies: an Option as `null`, a Redacted value
// revealed or masked, and a type with no wire form at all.

// nullOption is `option_as_null` (Effect's OptionFromNullOr): `null` is
// None and None is written `null`; `option_as_nullish` also takes a
// missing key as None.
type nullOption struct {
	item    codec
	nullish bool
}

func CodecOptionNull(item Value) Value    { return &nullOption{codecOf(item), false} }
func CodecOptionNullish(item Value) Value { return &nullOption{codecOf(item), true} }

func (n *nullOption) accepts(in Value) bool { return in == JsonNull || n.item.accepts(in) }

func (n *nullOption) decode(fb *Fiber, in Value) (Value, []issue) {
	if in == JsonNull {
		return None, nil
	}
	if !n.item.accepts(in) {
		return nil, expected("Union", in)
	}
	v, issues := n.item.decode(fb, in)
	if issues != nil {
		return nil, issues
	}
	return Some(v), nil
}

func (n *nullOption) encode(fb *Fiber, v Value) (Value, []issue) {
	x, ok := some(v)
	if !ok {
		return JsonNull, nil
	}
	return n.item.encode(fb, x)
}

// CodecRedactedReveal is `Redacted.json_reveal`: the content's wire form,
// an explicit opt-in.
func CodecRedactedReveal(item Value) Value {
	return &transformed{
		from: codecOf(item),
		to:   func(_ *Fiber, v Value) (Value, []issue) { return RedactedMake(v), nil },
		back: func(_ *Fiber, v Value) Value { return RedactedReveal(v) },
	}
}

// CodecRedactedMasked is `Redacted.json_masked`: written `"<redacted>"`,
// never decoded.
var CodecRedactedMasked Value = maskedRedacted{}

type maskedRedacted struct{}

const maskedText = "<redacted>"

func (maskedRedacted) accepts(in Value) bool { return in == maskedText }

func (maskedRedacted) decode(_ *Fiber, in Value) (Value, []issue) {
	if in != maskedText {
		return nil, expected(`"`+maskedText+`"`, in)
	}
	return nil, fail("a masked Redacted value cannot be decoded")
}

func (maskedRedacted) encode(*Fiber, Value) (Value, []issue) { return maskedText, nil }

// refused is the wire codec of a type with no wire form: both ways fail,
// and so does a JSON Schema export, with `reason`.
type refused struct{ reason string }

func (refused) accepts(Value) bool                      { return true }
func (r refused) decode(*Fiber, Value) (Value, []issue) { return nil, fail(r.reason) }
func (r refused) encode(*Fiber, Value) (Value, []issue) { return nil, fail(r.reason) }

// SchemaOpaque is `Schema.opaque name reason`: any value checks, and its
// JSON codec is refused with `reason`; no issue names it.
func SchemaOpaque(_, reason Value) Value {
	s := &Schema{kind: kindUnknown}
	s.wireOnce.Do(func() { s.wire = refused{reason.(string)} })
	return s
}
