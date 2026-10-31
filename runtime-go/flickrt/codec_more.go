package flickrt

import (
	"regexp"
	"slices"
	"strings"
)

// Codecs over codecs, as Effect Schema's structures and combinators: a
// dictionary as entries, lists of one or more, a codec the other way
// round, its two sides as schemas, fallbacks and runtime decoding.

// entries is `Codec.entries`: a JSON object as the list of its entries.
type entriesCodec struct{ value codec }

func CodecEntries(value Value) Value { return &entriesCodec{codecOf(value)} }

func (e *entriesCodec) accepts(in Value) bool { _, ok := in.(*Record); return ok }

func (e *entriesCodec) decode(fb *Fiber, in Value) (Value, []issue) {
	obj, ok := in.(*Record)
	if !ok {
		return nil, expected("an object", in)
	}
	out := make([]Value, len(obj.keys))
	for i, k := range obj.keys {
		v, issues := e.value.decode(fb, obj.vals[i])
		if issues != nil {
			return nil, under(k, issues)
		}
		out[i] = []Value{k, v}
	}
	return out, nil
}

func (e *entriesCodec) encode(fb *Fiber, v Value) (Value, []issue) {
	out := newRecord(0)
	for _, entry := range List(v) {
		pair := List(entry)
		k := pair[0].(string)
		w, issues := e.value.encode(fb, pair[1])
		if issues != nil {
			return nil, under(k, issues)
		}
		out.put(k, w)
	}
	return out, nil
}

func (e *entriesCodec) jsonSchema(x *schemaExport, path string) Value {
	return Rec("type", "object", "additionalProperties", x.codec(e.value, path+".*"))
}

// nonEmptyList is `Codec.non_empty_list`, Effect's NonEmptyArray.
type nonEmptyList struct{ listCodec }

func CodecNonEmptyList(item Value) Value { return &nonEmptyList{listCodec{codecOf(item)}} }

func (n *nonEmptyList) decode(fb *Fiber, in Value) (Value, []issue) {
	if xs, ok := in.([]Value); ok && len(xs) == 0 {
		return nil, under(0, fail("a missing key"))
	}
	return n.listCodec.decode(fb, in)
}

func (n *nonEmptyList) jsonSchema(x *schemaExport, path string) Value {
	return Rec("type", "array", "prefixItems", []Value{x.codec(n.item, path+"[0]")},
		"items", x.codec(n.item, path+"[]"), "minItems", float64(1))
}

// listEnsure is `Codec.list_ensure`, Effect's ArrayEnsure: the union of an
// element and an array, a list of one written as its element.
type listEnsure struct{ item codec }

func CodecListEnsure(item Value) Value { return &listEnsure{codecOf(item)} }

func (l *listEnsure) accepts(in Value) bool {
	_, isArray := in.([]Value)
	return isArray || l.item.accepts(in)
}

func (l *listEnsure) decode(fb *Fiber, in Value) (Value, []issue) {
	var all []issue
	if l.item.accepts(in) {
		v, issues := l.item.decode(fb, in)
		if issues == nil {
			return []Value{v}, nil
		}
		all = issues
	}
	if _, isArray := in.([]Value); isArray {
		v, issues := (&listCodec{l.item}).decode(fb, in)
		if issues == nil {
			return v, nil
		}
		all = append(all, issues...)
	}
	if all == nil {
		return nil, expected("Union", in)
	}
	return nil, all
}

func (l *listEnsure) encode(fb *Fiber, v Value) (Value, []issue) {
	if xs := List(v); len(xs) == 1 {
		return l.item.encode(fb, xs[0])
	}
	return (&listCodec{l.item}).encode(fb, v)
}

func (l *listEnsure) jsonSchema(x *schemaExport, path string) Value {
	return Rec("anyOf", []Value{x.codec(l.item, path), Rec("type", "array", "items", x.codec(l.item, path+"[]"))})
}

// flipped is `Codec.flip`: decoding encodes and encoding decodes.
//
// shortcut: decoding hands the input to the codec's encoder, which takes it to be a value of the decoded type, as Effect checks first; a value of another type is a defect
type flipped struct{ c codec }

func CodecFlip(c Value) Value {
	if f, ok := c.(*flipped); ok {
		return f.c
	}
	return &flipped{codecOf(c)}
}

func (f *flipped) accepts(Value) bool                          { return true }
func (f *flipped) decode(fb *Fiber, in Value) (Value, []issue) { return f.c.encode(fb, in) }
func (f *flipped) encode(fb *Fiber, v Value) (Value, []issue)  { return f.c.decode(fb, v) }

// CodecToSchema is `Codec.to_schema`: the decoded side, whose canonical
// JSON codec is `c`.
//
// shortcut: Schema.validate of it accepts what the codec encodes, the go target keeps no type of a codec's decoded side
func CodecToSchema(c Value) Value {
	k := codecOf(c)
	return customSchema(k, func(fb *Fiber, v Value) (Value, []issue) {
		if _, issues := k.encode(fb, v); issues != nil {
			return nil, issues
		}
		return v, nil
	})
}

// wireOnly is a codec's wire values as themselves.
//
// shortcut: a wire value is one the codec decodes, a stricter test than Effect's encoded side when decoding refuses a value of the right JSON type
type wireOnly struct{ c codec }

func (w *wireOnly) accepts(in Value) bool { return w.c.accepts(in) }
func (w *wireOnly) decode(fb *Fiber, in Value) (Value, []issue) {
	if _, issues := w.c.decode(fb, in); issues != nil {
		return nil, issues
	}
	return in, nil
}
func (w *wireOnly) encode(fb *Fiber, v Value) (Value, []issue) { return w.decode(fb, v) }
func (w *wireOnly) jsonSchema(x *schemaExport, path string) Value {
	return x.codec(w.c, path)
}

// CodecWireSchema is `Codec.wire_schema`: the wire side, written as itself.
func CodecWireSchema(c Value) Value {
	w := &wireOnly{codecOf(c)}
	return customSchema(w, w.decode)
}

// fallbackCodec is a codec whose failures are given to a fallback, which
// gets the SchemaError; `refusal` is why export refuses it, or "".
type fallbackCodec struct {
	from               codec
	onDecode, onEncode func(fb *Fiber, err Value) (Value, []issue)
	refusal            string
}

func (r *fallbackCodec) accepts(in Value) bool { return r.from.accepts(in) }

func (r *fallbackCodec) decode(fb *Fiber, in Value) (Value, []issue) {
	v, issues := r.from.decode(fb, in)
	if issues != nil && r.onDecode != nil {
		return r.onDecode(fb, schemaErrorOf(issues))
	}
	return v, issues
}

func (r *fallbackCodec) encode(fb *Fiber, v Value) (Value, []issue) {
	w, issues := r.from.encode(fb, v)
	if issues != nil && r.onEncode != nil {
		return r.onEncode(fb, schemaErrorOf(issues))
	}
	return w, issues
}

func (r *fallbackCodec) jsonSchema(x *schemaExport, path string) Value {
	if r.refusal != "" {
		refuseExport(path, r.refusal)
	}
	return x.codec(r.from, path)
}

const refuseFallback = "a decoding fallback accepts input the wire schema does not state"

// resultFallback is a fallback by a function to a Result.
func resultFallback(f Value) func(*Fiber, Value) (Value, []issue) {
	return func(fb *Fiber, err Value) (Value, []issue) {
		r := Apply(fb, f, err)
		if !isOk(r) {
			return nil, issuesOf(value(r))
		}
		return value(r), nil
	}
}

// taskFallback is a fallback by a function to a task.
func taskFallback(f Value) func(*Fiber, Value) (Value, []issue) {
	return func(fb *Fiber, err Value) (Value, []issue) { return schemaTask(fb, Apply(fb, f, err)) }
}

func CodecCatchDecoding(f, c Value) Value {
	return &fallbackCodec{from: codecOf(c), onDecode: resultFallback(f), refusal: refuseFallback}
}

func CodecCatchDecodingEffect(f, c Value) Value {
	return &fallbackCodec{from: codecOf(c), onDecode: taskFallback(f), refusal: refuseFallback}
}

func CodecCatchEncoding(f, c Value) Value {
	return &fallbackCodec{from: codecOf(c), onEncode: resultFallback(f)}
}

func CodecCatchEncodingEffect(f, c Value) Value {
	return &fallbackCodec{from: codecOf(c), onEncode: taskFallback(f)}
}

// schemaTask runs a task of the program; its SchemaError is the issue.
func schemaTask(fb *Fiber, task Value) (Value, []issue) {
	v, cause := catch(func() Value { return Run(fb, task) })
	if cause == nil {
		return v, nil
	}
	if d, ok := cause.err.(*Data); ok && cause.kind == causeFail && d.Tag == schemaErrorTag {
		return nil, issuesOf(d)
	}
	panic(&Failure{cause})
}

const refuseEffect = "an effectful transformation decides acceptance at run time"

// CodecTransformEffect is `Codec.transform_effect`: decoding is a task.
func CodecTransformEffect(decode, encode, from Value) Value {
	return &transformed{
		from:    codecOf(from),
		to:      func(fb *Fiber, v Value) (Value, []issue) { return schemaTask(fb, Apply(fb, decode, v)) },
		back:    func(fb *Fiber, v Value) Value { return Apply(fb, encode, v) },
		refusal: refuseEffect,
	}
}

// CodecCheckEffect is `Codec.check_effect`: a task decides whether the
// decoded value passes.
func CodecCheckEffect(f, c Value) Value {
	return &transformed{
		from: codecOf(c),
		to: func(fb *Fiber, v Value) (Value, []issue) {
			ok, issues := schemaTask(fb, Apply(fb, f, v))
			if issues == nil && !ok.(bool) {
				issues = fail("an invalid value")
			}
			if issues != nil {
				return nil, issues
			}
			return v, nil
		},
		back:    func(_ *Fiber, v Value) Value { return v },
		refusal: refuseEffect,
	}
}

// CodecEncodeXml is `Codec.encode_xml`: the encoded value as Effect's
// pretty XML under the codec's title, or `root`, keys sorted.
//
// shortcut: the leaves are the JS text of the JSON encoding, which is Effect's string tree for JSON codecs; port toCodecStringTree if a codec writes leaves another way
func CodecEncodeXml(fb *Fiber, c, v Value) Value {
	w, issues := codecOf(c).encode(fb, v)
	if issues != nil {
		return Fail(schemaErrorOf(issues))
	}
	root := "root"
	if k, ok := c.(*checkedCodec); ok && isSome(k.title) {
		root = value(k.title).(string)
	}
	var lines []string
	xmlNode(&lines, root, w, 0, "")
	return strings.Join(lines, "\n")
}

var (
	xmlStart     = regexp.MustCompile(`^[A-Za-z_]`)
	xmlUnsafe    = regexp.MustCompile(`[^A-Za-z0-9._-]`)
	xmlPrefix    = regexp.MustCompile(`(?i)^xml`)
	xmlText      = strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;")
	xmlAttribute = strings.NewReplacer("&", "&amp;", `"`, "&quot;", "<", "&lt;", ">", "&gt;")
)

func xmlName(name string) string {
	safe := name
	if !xmlStart.MatchString(safe) {
		safe = "_" + safe
	}
	safe = xmlUnsafe.ReplaceAllString(safe, "_")
	if xmlPrefix.MatchString(safe) {
		safe = "_" + safe
	}
	return safe
}

func xmlNode(lines *[]string, name string, node Value, depth int, original string) {
	safe, attrs := xmlName(name), ""
	if safe != name || original != "" && original != name {
		if original == "" {
			original = name
		}
		attrs = ` data-name="` + xmlAttribute.Replace(original) + `"`
	}
	push := func(text string) { *lines = append(*lines, strings.Repeat("  ", depth)+text) }
	switch x := node.(type) {
	case *jsonNull:
		push("<" + safe + attrs + "/>")
	case []Value:
		if len(x) == 0 {
			push("<" + safe + attrs + "/>")
			return
		}
		push("<" + safe + attrs + ">")
		for _, e := range x {
			xmlNode(lines, "item", e, depth+1, "")
		}
		push("</" + safe + ">")
	case *Record:
		if len(x.keys) == 0 {
			push("<" + safe + attrs + "/>")
			return
		}
		push("<" + safe + attrs + ">")
		keys := slices.Clone(x.keys)
		slices.Sort(keys)
		for _, k := range keys {
			xmlNode(lines, xmlName(k), recordField(x, k), depth+1, k)
		}
		push("</" + safe + ">")
	default:
		push("<" + safe + attrs + ">" + xmlText.Replace(jsString(x)) + "</" + safe + ">")
	}
}
