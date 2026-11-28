package flickrt

import (
	"math"
	"math/big"
	"strconv"
)

var (
	flnUnitTag        = Tag("Fln.Value.Unit")
	flnBoolTag        = Tag("Fln.Value.Bool")
	flnIntTag         = Tag("Fln.Value.Int")
	flnBigIntTag      = Tag("Fln.Value.BigInt")
	flnFloatTag       = Tag("Fln.Value.Float")
	flnStringTag      = Tag("Fln.Value.String")
	flnTupleTag       = Tag("Fln.Value.Tuple")
	flnListTag        = Tag("Fln.Value.List")
	flnRecordTag      = Tag("Fln.Value.Record")
	flnConstructorTag = Tag("Fln.Value.Constructor")

	flnSufI8Tag  = Tag("Fln.NumSuffix.I8")
	flnSufI16Tag = Tag("Fln.NumSuffix.I16")
	flnSufI32Tag = Tag("Fln.NumSuffix.I32")
	flnSufI64Tag = Tag("Fln.NumSuffix.I64")
	flnSufU8Tag  = Tag("Fln.NumSuffix.U8")
	flnSufU16Tag = Tag("Fln.NumSuffix.U16")
	flnSufU32Tag = Tag("Fln.NumSuffix.U32")
	flnSufU64Tag = Tag("Fln.NumSuffix.U64")
	flnSufF32Tag = Tag("Fln.NumSuffix.F32")
)

func SchemaFlnCodec(s Value) Value { return &flnCodec{s: sch(s)} }

type flnCodec struct{ s *Schema }

func (c *flnCodec) accepts(in Value) bool {
	_, ok := in.(*Data)
	return ok
}

func flnSuffixTag(opt Value) string {
	if !IsSome(opt) {
		return ""
	}
	if d, ok := UnwrapSome(opt).(*Data); ok {
		return d.Tag
	}
	return ""
}

func describeFlnValue(v Value) string {
	d, ok := v.(*Data)
	if !ok {
		return "a non-FLN value"
	}
	switch d.Tag {
	case flnUnitTag:
		return "()"
	case flnBoolTag:
		return "Bool"
	case flnIntTag:
		return "Int"
	case flnBigIntTag:
		return "BigInt"
	case flnFloatTag:
		return "Float"
	case flnStringTag:
		return "String"
	case flnTupleTag:
		return "Tuple"
	case flnListTag:
		return "List"
	case flnRecordTag:
		return "Record"
	case flnConstructorTag:
		if len(d.Fields()) >= 2 {
			if name, ok := d.Fields()[1].(string); ok {
				return name
			}
		}
		return "Constructor"
	}
	return "a non-FLN value"
}

func flnOptionSuffix(sufTag string) Value {
	if sufTag == "" {
		return None
	}
	return Some(Nullary(sufTag))
}

func flnDecodeBoundedInt(in Value, label, wantSuf string, min, max *big.Int) (*big.Int, []issue) {
	d, ok := in.(*Data)
	if !ok || d.Tag != flnIntTag || len(d.Fields()) < 2 {
		return nil, fail("expected " + label + ", got " + describeFlnValue(in))
	}
	n, ok := d.Fields()[0].(*big.Int)
	if !ok {
		return nil, fail("expected " + label + ", got " + describeFlnValue(in))
	}
	suf := flnSuffixTag(d.Fields()[1])
	if suf != "" && suf != wantSuf {
		return nil, fail("expected " + label + " suffix, got mismatched numeric suffix")
	}
	if n.Cmp(min) < 0 || n.Cmp(max) > 0 {
		return nil, fail("expected " + label + ", an integer from " + min.String() + " to " + max.String())
	}
	return n, nil
}

var (
	minI8Big  = big.NewInt(-128)
	maxI8Big  = big.NewInt(127)
	minI16Big = big.NewInt(-32768)
	maxI16Big = big.NewInt(32767)
	minI32Big = big.NewInt(math.MinInt32)
	maxI32Big = big.NewInt(math.MaxInt32)
	minI64Big = big.NewInt(math.MinInt64)
	maxI64Big = big.NewInt(math.MaxInt64)
	zeroBig   = big.NewInt(0)
	maxU8Big  = big.NewInt(255)
	maxU16Big = big.NewInt(65535)
	maxU32Big = big.NewInt(math.MaxUint32)
	maxU64Big = new(big.Int).SetUint64(math.MaxUint64)
)

func (c *flnCodec) decode(fb *Fiber, in Value) (Value, []issue) {
	return decodeFlnSchema(fb, c.s, in)
}

func (c *flnCodec) encode(fb *Fiber, v Value) (Value, []issue) {
	return encodeFlnSchema(fb, c.s, v)
}

func decodeFlnSchema(fb *Fiber, s *Schema, in Value) (Value, []issue) {
	s = s.resolved()
	switch s.kind {
	case kindUnit:
		if d, ok := in.(*Data); ok && d.Tag == flnUnitTag {
			return Unit, nil
		}
		return nil, fail("expected (), got " + describeFlnValue(in))
	case kindBool:
		if d, ok := in.(*Data); ok && d.Tag == flnBoolTag {
			return d.Fields()[0].(bool), nil
		}
		return nil, fail("expected a boolean, got " + describeFlnValue(in))
	case kindString:
		if d, ok := in.(*Data); ok && d.Tag == flnStringTag {
			return d.Fields()[0].(string), nil
		}
		return nil, fail("expected a string, got " + describeFlnValue(in))
	case kindInt:
		n, issues := flnDecodeBoundedInt(in, "an Int", flnSufI32Tag, minI32Big, maxI32Big)
		if issues != nil {
			return nil, issues
		}
		return int32(n.Int64()), nil
	case kindInt8:
		n, issues := flnDecodeBoundedInt(in, "an Int8", flnSufI8Tag, minI8Big, maxI8Big)
		if issues != nil {
			return nil, issues
		}
		return int8(n.Int64()), nil
	case kindInt16:
		n, issues := flnDecodeBoundedInt(in, "an Int16", flnSufI16Tag, minI16Big, maxI16Big)
		if issues != nil {
			return nil, issues
		}
		return int16(n.Int64()), nil
	case kindInt64:
		n, issues := flnDecodeBoundedInt(in, "an Int64", flnSufI64Tag, minI64Big, maxI64Big)
		if issues != nil {
			return nil, issues
		}
		return n.Int64(), nil
	case kindUInt8:
		n, issues := flnDecodeBoundedInt(in, "a UInt8", flnSufU8Tag, zeroBig, maxU8Big)
		if issues != nil {
			return nil, issues
		}
		return uint8(n.Uint64()), nil
	case kindUInt16:
		n, issues := flnDecodeBoundedInt(in, "a UInt16", flnSufU16Tag, zeroBig, maxU16Big)
		if issues != nil {
			return nil, issues
		}
		return uint16(n.Uint64()), nil
	case kindUInt32:
		n, issues := flnDecodeBoundedInt(in, "a UInt32", flnSufU32Tag, zeroBig, maxU32Big)
		if issues != nil {
			return nil, issues
		}
		return uint32(n.Uint64()), nil
	case kindUInt64:
		n, issues := flnDecodeBoundedInt(in, "a UInt64", flnSufU64Tag, zeroBig, maxU64Big)
		if issues != nil {
			return nil, issues
		}
		return n.Uint64(), nil
	case kindBigint:
		if d, ok := in.(*Data); ok && d.Tag == flnBigIntTag {
			return d.Fields()[0].(*big.Int), nil
		}
		return nil, fail("expected a BigInt, got " + describeFlnValue(in))
	case kindNumber:
		if d, ok := in.(*Data); ok {
			if d.Tag == flnFloatTag && flnSuffixTag(d.Fields()[1]) == "" {
				x := d.Fields()[0].(float64)
				if !math.IsNaN(x) && !math.IsInf(x, 0) {
					return x, nil
				}
				return nil, fail("expected a finite number")
			}
			if d.Tag == flnIntTag && flnSuffixTag(d.Fields()[1]) == "" {
				x, _ := new(big.Float).SetInt(d.Fields()[0].(*big.Int)).Float64()
				if !math.IsNaN(x) && !math.IsInf(x, 0) {
					return x, nil
				}
				return nil, fail("expected a finite number")
			}
		}
		return nil, fail("expected a finite number, got " + describeFlnValue(in))
	case kindFloat32:
		if d, ok := in.(*Data); ok {
			if d.Tag == flnFloatTag {
				suf := flnSuffixTag(d.Fields()[1])
				if suf == "" || suf == flnSufF32Tag {
					f := float32(d.Fields()[0].(float64))
					if !math.IsNaN(float64(f)) && !math.IsInf(float64(f), 0) {
						return f, nil
					}
					return nil, fail("expected a finite Float32")
				}
			}
			if d.Tag == flnIntTag && flnSuffixTag(d.Fields()[1]) == "" {
				f := ToFloat32(d.Fields()[0]).(float32)
				if !math.IsNaN(float64(f)) && !math.IsInf(float64(f), 0) {
					return f, nil
				}
				return nil, fail("expected a finite Float32")
			}
		}
		return nil, fail("expected a Float32, got " + describeFlnValue(in))
	case kindNever:
		return nil, fail("expected nothing (Never), got " + describeFlnValue(in))
	case kindUnknown:
		return in, nil
	case kindRedacted:
		return nil, fail("a Redacted value has no FLN form without an explicit policy")
	case kindList:
		d, ok := in.(*Data)
		if !ok || d.Tag != flnListTag {
			return nil, fail("expected a list, got " + describeFlnValue(in))
		}
		xs := List(d.Fields()[0])
		out := make([]Value, len(xs))
		for i, x := range xs {
			v, issues := decodeFlnSchema(fb, s.item, x)
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = v
		}
		return out, nil
	case kindTuple:
		d, ok := in.(*Data)
		if !ok || d.Tag != flnTupleTag {
			return nil, fail("expected a tuple of " + strconv.Itoa(len(s.items)) + ", got " + describeFlnValue(in))
		}
		xs := List(d.Fields()[0])
		out := make([]Value, len(s.items))
		for i, item := range s.items {
			if i >= len(xs) {
				return nil, under(i, fail("a missing tuple item"))
			}
			v, issues := decodeFlnSchema(fb, item, xs[i])
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = v
		}
		if len(xs) > len(s.items) {
			return nil, under(len(s.items), fail("an extra tuple item"))
		}
		return out, nil
	case kindOption:
		d, ok := in.(*Data)
		if !ok || d.Tag != flnConstructorTag {
			return nil, fail("expected None or Some, got " + describeFlnValue(in))
		}
		name := d.Fields()[1].(string)
		args := List(d.Fields()[2])
		if name == "None" && len(args) == 0 {
			return None, nil
		}
		if name == "Some" && len(args) == 1 {
			v, issues := decodeFlnSchema(fb, s.item, args[0])
			if issues != nil {
				return nil, issues
			}
			return Some(v), nil
		}
		return nil, fail("expected None or Some, got " + name)
	case kindResult:
		d, ok := in.(*Data)
		if !ok || d.Tag != flnConstructorTag {
			return nil, fail("expected Ok or Err, got " + describeFlnValue(in))
		}
		name := d.Fields()[1].(string)
		args := List(d.Fields()[2])
		if name == "Ok" && len(args) == 1 {
			v, issues := decodeFlnSchema(fb, s.items[1], args[0])
			if issues != nil {
				return nil, under("ok", issues)
			}
			return Ok(v), nil
		}
		if name == "Err" && len(args) == 1 {
			v, issues := decodeFlnSchema(fb, s.items[0], args[0])
			if issues != nil {
				return nil, under("err", issues)
			}
			return Err(v), nil
		}
		return nil, fail("expected Ok or Err, got " + name)
	case kindRecord:
		d, ok := in.(*Data)
		if !ok || d.Tag != flnRecordTag {
			return nil, fail("expected a record, got " + describeFlnValue(in))
		}
		pairs := List(d.Fields()[0])
		byKey := make(map[string]Value, len(pairs))
		order := make([]string, 0, len(pairs))
		for _, p := range pairs {
			kv := List(p)
			k := kv[0].(string)
			byKey[k] = kv[1]
			order = append(order, k)
		}
		fields := make([]Value, len(s.labels))
		for i, label := range s.labels {
			val, present := byKey[label]
			if !present {
				return nil, under(label, fail("a missing key"))
			}
			v, issues := decodeFlnSchema(fb, s.items[i], val)
			if issues != nil {
				return nil, under(label, issues)
			}
			fields[i] = v
		}
		for _, k := range order {
			known := false
			for _, label := range s.labels {
				if k == label {
					known = true
					break
				}
			}
			if !known {
				return nil, under(k, fail("an unknown key"))
			}
		}
		return shaped(s.keys, s.pos, fields), nil
	case kindAdt:
		d, ok := in.(*Data)
		if !ok || d.Tag != flnConstructorTag {
			return nil, fail("expected a constructor, got " + describeFlnValue(in))
		}
		name := d.Fields()[1].(string)
		args := List(d.Fields()[2])
		for at, v := range s.variants {
			if v.Name != name {
				continue
			}
			if len(args) != len(v.Fields) {
				return nil, fail("constructor " + name + " expects " + strconv.Itoa(len(v.Fields)) + " argument(s), got " + strconv.Itoa(len(args)))
			}
			out := make([]Value, len(v.Fields))
			for i, fs := range s.fields[at][1:] {
				dec, issues := decodeFlnSchema(fb, fs, args[i])
				if issues != nil {
					return nil, under("_"+strconv.Itoa(i), issues)
				}
				out[i] = dec
			}
			return Construct(v.Tag, out...), nil
		}
		names := ""
		for i, v := range s.variants {
			if i > 0 {
				names += ", "
			}
			names += v.Name
		}
		return nil, fail("expected one of " + names)
	case kindRefined:
		v, issues := decodeFlnSchema(fb, s.item, in)
		if issues == nil {
			issues = s.refine(fb, v)
		}
		return v, issues
	case kindCustom:
		if e, ok := s.jsonOf().(*enumCodec); ok {
			if e.adt != nil {
				d, ok := in.(*Data)
				if !ok || d.Tag != flnConstructorTag || len(List(d.Fields()[2])) != 0 {
					return nil, fail("expected an enum constructor, got " + describeFlnValue(in))
				}
				name := d.Fields()[1].(string)
				for i, v := range e.adt.variants {
					if v.Name == name {
						return e.values[i], nil
					}
				}
				return nil, fail("unknown enum constructor " + name)
			}
			v, issues := decodeFlnSchema(fb, e.schema, in)
			if issues != nil {
				return nil, issues
			}
			if e.index(v) < 0 {
				return nil, fail("expected an enum literal")
			}
			return v, nil
		}
		return nil, fail("schema has no canonical FLN form")
	}
	return nil, fail("schema has no canonical FLN form")
}

func encodeFlnSchema(fb *Fiber, s *Schema, v Value) (Value, []issue) {
	s = s.resolved()
	switch s.kind {
	case kindUnit:
		return Nullary(flnUnitTag), nil
	case kindBool:
		return NewData(flnBoolTag, v.(bool)), nil
	case kindString:
		return NewData(flnStringTag, v.(string)), nil
	case kindInt:
		return NewData(flnIntTag, big.NewInt(int64(v.(int32))), None), nil
	case kindInt8:
		return NewData(flnIntTag, big.NewInt(int64(v.(int8))), flnOptionSuffix(flnSufI8Tag)), nil
	case kindInt16:
		return NewData(flnIntTag, big.NewInt(int64(v.(int16))), flnOptionSuffix(flnSufI16Tag)), nil
	case kindInt64:
		return NewData(flnIntTag, big.NewInt(v.(int64)), flnOptionSuffix(flnSufI64Tag)), nil
	case kindUInt8:
		return NewData(flnIntTag, new(big.Int).SetUint64(uint64(v.(uint8))), flnOptionSuffix(flnSufU8Tag)), nil
	case kindUInt16:
		return NewData(flnIntTag, new(big.Int).SetUint64(uint64(v.(uint16))), flnOptionSuffix(flnSufU16Tag)), nil
	case kindUInt32:
		return NewData(flnIntTag, new(big.Int).SetUint64(uint64(v.(uint32))), flnOptionSuffix(flnSufU32Tag)), nil
	case kindUInt64:
		return NewData(flnIntTag, new(big.Int).SetUint64(v.(uint64)), flnOptionSuffix(flnSufU64Tag)), nil
	case kindBigint:
		return NewData(flnBigIntTag, v.(*big.Int)), nil
	case kindNumber:
		x := v.(float64)
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return nil, fail(ShowNumber(x) + " has no FLN form")
		}
		return NewData(flnFloatTag, x, None), nil
	case kindFloat32:
		x := float64(v.(float32))
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return nil, fail(ShowNumber(x) + " has no FLN form")
		}
		return NewData(flnFloatTag, x, flnOptionSuffix(flnSufF32Tag)), nil
	case kindNever:
		return nil, fail("expected nothing (Never)")
	case kindUnknown:
		return v, nil
	case kindRedacted:
		return nil, fail("a Redacted value has no FLN form without an explicit policy")
	case kindList:
		xs := List(v)
		out := make([]Value, len(xs))
		for i, x := range xs {
			e, issues := encodeFlnSchema(fb, s.item, x)
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = e
		}
		return NewData(flnListTag, out), nil
	case kindTuple:
		xs := List(v)
		out := make([]Value, len(s.items))
		for i, item := range s.items {
			e, issues := encodeFlnSchema(fb, item, xs[i])
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = e
		}
		return NewData(flnTupleTag, out), nil
	case kindOption:
		if !IsSome(v) {
			return NewData(flnConstructorTag, []Value{}, "None", []Value{}), nil
		}
		e, issues := encodeFlnSchema(fb, s.item, UnwrapSome(v))
		if issues != nil {
			return nil, issues
		}
		return NewData(flnConstructorTag, []Value{}, "Some", []Value{e}), nil
	case kindResult:
		if isOk(v) {
			e, issues := encodeFlnSchema(fb, s.items[1], value(v))
			if issues != nil {
				return nil, under("ok", issues)
			}
			return NewData(flnConstructorTag, []Value{}, "Ok", []Value{e}), nil
		}
		e, issues := encodeFlnSchema(fb, s.items[0], value(v))
		if issues != nil {
			return nil, under("err", issues)
		}
		return NewData(flnConstructorTag, []Value{}, "Err", []Value{e}), nil
	case kindRecord:
		r := AsRecord(v)
		out := make([]Value, len(s.labels))
		for i, l := range s.labels {
			e, issues := encodeFlnSchema(fb, s.items[i], r.Get(l))
			if issues != nil {
				return nil, under(l, issues)
			}
			out[i] = []Value{l, e}
		}
		return NewData(flnRecordTag, out), nil
	case kindAdt:
		tag, fields := tagName(v), fieldsIn(v)
		for at, variant := range s.variants {
			if variant.Tag != tag {
				continue
			}
			args := make([]Value, len(variant.Fields))
			for i, fs := range s.fields[at][1:] {
				e, issues := encodeFlnSchema(fb, fs, fields[i])
				if issues != nil {
					return nil, under("_"+strconv.Itoa(i), issues)
				}
				args[i] = e
			}
			return NewData(flnConstructorTag, []Value{}, variant.Name, args), nil
		}
		return nil, fail("expected a variant")
	case kindRefined:
		if issues := s.refine(fb, v); issues != nil {
			return nil, issues
		}
		return encodeFlnSchema(fb, s.item, v)
	case kindCustom:
		if e, ok := s.jsonOf().(*enumCodec); ok {
			i := e.index(v)
			if i < 0 {
				return nil, fail("expected an enum value")
			}
			if e.adt != nil {
				return NewData(flnConstructorTag, []Value{}, e.adt.variants[i].Name, []Value{}), nil
			}
			return encodeFlnSchema(fb, e.schema, v)
		}
		return nil, fail("schema has no canonical FLN form")
	}
	return nil, fail("schema has no canonical FLN form")
}
