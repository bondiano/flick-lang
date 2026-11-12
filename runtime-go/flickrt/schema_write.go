package flickrt

import (
	"math"
	"math/big"
	"strconv"
	"strings"
)

// write writes the JSON text of `v` as stringifying its encode would, without
// building the JSON value first; on an issue what it wrote is to be dropped.
func (w *wire) write(fb *Fiber, b *strings.Builder, v Value) []issue {
	s := w.s.resolved()
	switch s.kind {
	case kindRefined:
		if issues := s.refine(fb, v); issues != nil {
			return issues
		}
		return s.item.jsonOf().(*wire).write(fb, b, v)
	case kindNumber:
		x := v.(float64)
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return fail(ShowNumber(x) + " has no JSON form")
		}
		if x == 0 {
			b.WriteByte('0')
		} else {
			b.WriteString(ShowNumber(x))
		}
	case kindInt:
		b.WriteString(strconv.Itoa(int(v.(int32))))
	case kindBigint:
		quoteTo(b, v.(*big.Int).String())
	case kindString:
		quoteTo(b, v.(string))
	case kindBool:
		b.WriteString(strconv.FormatBool(v.(bool)))
	case kindUnit:
		b.WriteString("null")
	case kindList, kindTuple:
		b.WriteByte('[')
		for i, x := range List(v) {
			if i > 0 {
				b.WriteByte(',')
			}
			item := s.item
			if s.kind == kindTuple {
				item = s.items[i]
			}
			if issues := item.jsonOf().(*wire).write(fb, b, x); issues != nil {
				return under(i, issues)
			}
		}
		b.WriteByte(']')
	case kindOption:
		o := v.(*Data)
		if o.Tag != "Some" {
			b.WriteString("null")
			return nil
		}
		return s.item.jsonOf().(*wire).write(fb, b, o.Fields()[0])
	case kindRecord:
		if s.pos != nil {
			return w.writeEncoded(fb, b, v)
		}
		r := v.(*Record)
		b.WriteByte('{')
		for i, l := range s.labels {
			if i > 0 {
				b.WriteByte(',')
			}
			quoteTo(b, l)
			b.WriteByte(':')
			if issues := s.items[i].jsonOf().(*wire).write(fb, b, r.Get(l)); issues != nil {
				return under(l, issues)
			}
		}
		b.WriteByte('}')
	case kindAdt:
		tag, fields := tagName(v), fieldsIn(v)
		for at, variant := range s.variants {
			if variant.Tag != tag {
				continue
			}
			keys := s.wireKeys[at]
			b.WriteString(`{"_tag":`)
			quoteTo(b, variant.Name)
			for i, f := range s.fields[at][1:] {
				b.WriteByte(',')
				quoteTo(b, keys[i+1])
				b.WriteByte(':')
				if issues := f.jsonOf().(*wire).write(fb, b, fields[i]); issues != nil {
					return under(keys[i+1], issues)
				}
			}
			b.WriteByte('}')
			return nil
		}
		return w.writeEncoded(fb, b, v)
	default:
		return w.writeEncoded(fb, b, v)
	}
	return nil
}

// writeEncoded writes `v` by way of its JSON value.
func (w *wire) writeEncoded(fb *Fiber, b *strings.Builder, v Value) []issue {
	e, issues := w.encode(fb, v)
	if issues != nil {
		return issues
	}
	stringify(b, e)
	return nil
}
