package flickrt

import (
	"strconv"
	"strings"
)

// The strict JSON Schema 2020-12 exporter, as the JS runtime's over the
// AST of a codec: the wire contract written exactly, or an ExportError
// naming the path of the part that has no exact form. Never `{}` or `true`
// for something it cannot state.

var exportErrorTag = Tag("Codec.ExportError.ExportError")

// schemaExport writes the schemas of codecs and the definitions of the
// recursive types they refer to, by name.
type schemaExport struct{ defs *Record }

// exportRefused is the message of an ExportError, raised while writing.
type exportRefused string

func refuseExport(path, reason string) {
	panic(exportRefused(path + " -> JSON Schema: " + reason))
}

// atPath is the path of `key` under `path`.
func atPath(path, key string) string {
	if path == "" {
		return key
	}
	return path + "." + key
}

// closedObject is an object of the properties, all required, and no other.
func closedObject(keys []string, schemas []Value, closed bool) Value {
	props := newRecord(len(keys))
	required := make([]Value, len(keys))
	for i, k := range keys {
		props.put(k, schemas[i])
		required[i] = k
	}
	if !closed {
		return Rec("type", "object", "properties", props, "required", required)
	}
	return Rec("type", "object", "properties", props, "required", required, "additionalProperties", false)
}

func (x *schemaExport) codec(c codec, path string) Value {
	switch k := c.(type) {
	case *wire:
		return x.schema(k.s, path)
	case *block:
		keys, schemas := make([]string, len(k.fields)), make([]Value, len(k.fields))
		for i, f := range k.fields {
			keys[i], schemas[i] = f.wire, x.codec(f.codec, atPath(path, f.wire))
		}
		return closedObject(keys, schemas, !k.ignoreExtra)
	case *listCodec:
		return Rec("type", "array", "items", x.codec(k.item, path+"[]"))
	case *taggedOption:
		return Rec("oneOf", []Value{
			closedObject([]string{"_tag"}, []Value{Rec("const", "None")}, true),
			closedObject([]string{"_tag", "value"}, []Value{Rec("const", "Some"), x.codec(k.item, atPath(path, "value"))}, true),
		})
	case losslessNumber:
		return Rec("anyOf", []Value{Rec("type", "number"), Rec("enum", []Value{"NaN", "Infinity", "-Infinity", "-0"})})
	case *transformed:
		if k.refusal != "" {
			refuseExport(path, k.refusal)
		}
		return x.codec(k.from, path)
	case *fallible:
		refuseExport(path, "a fallible transformation has no JSON Schema form")
	}
	// shortcut: the string-tree, parameter and HTTP value codecs are refused; write their forms when a program exports one
	refuseExport(path, "this codec has no JSON Schema form on the go target")
	return nil
}

var int32Bounds = []Value{"minimum", float64(-0x80000000), "maximum", float64(0x7fffffff)}

func (x *schemaExport) schema(s *Schema, path string) Value {
	switch s.kind {
	case kindLazy:
		s.resolved()
		if x.defs.index(s.name) < 0 {
			x.defs.put(s.name, nil)
			x.defs.put(s.name, x.schema(s.target, path))
		}
		return Rec("$ref", "#/$defs/"+s.name)
	case kindNumber:
		return Rec("type", "number")
	case kindInt:
		return Rec(append([]Value{"type", "integer"}, int32Bounds...)...)
	case kindString:
		return Rec("type", "string")
	case kindBool:
		return Rec("type", "boolean")
	case kindBigint:
		return Rec("type", "string", "pattern", "^-?[0-9]+$")
	case kindUnit:
		return Rec("type", "null")
	case kindNever:
		return false
	case kindUnknown:
		refuseExport(path, "an Unknown value accepts anything, which strict export does not write")
	case kindRedacted:
		refuseExport(path, redactedRefusal)
	case kindList:
		return Rec("type", "array", "items", x.schema(s.item, path+"[]"))
	case kindTuple:
		items := make([]Value, len(s.items))
		for i, item := range s.items {
			items[i] = x.schema(item, path+"["+strconv.Itoa(i)+"]")
		}
		return Rec("type", "array", "prefixItems", items, "items", false)
	case kindOption:
		return Rec("anyOf", []Value{x.schema(s.item, path), Rec("type", "null")})
	case kindResult:
		return Rec("oneOf", []Value{
			closedObject([]string{"ok"}, []Value{x.schema(s.items[1], atPath(path, "ok"))}, true),
			closedObject([]string{"err"}, []Value{x.schema(s.items[0], atPath(path, "err"))}, true),
		})
	case kindRecord:
		schemas := make([]Value, len(s.keys))
		for i, k := range s.keys {
			for j, l := range s.labels {
				if l == k {
					schemas[i] = x.schema(s.items[j], atPath(path, k))
				}
			}
		}
		return closedObject(s.keys, schemas, true)
	case kindAdt:
		if len(s.variants) == 0 {
			return Rec("enum", []Value{})
		}
		members := make([]Value, len(s.variants))
		for at, v := range s.variants {
			keys := s.wireKeys[at]
			schemas := []Value{Rec("const", v.Name)}
			for i, f := range s.fields[at][1:] {
				schemas = append(schemas, x.schema(f, atPath(atPath(path, v.Name), keys[i+1])))
			}
			members[at] = closedObject(keys, schemas, true)
		}
		return Rec("oneOf", members)
	}
	panic(Defect{"a schema of no kind"})
}

// exportSchema is the schema of the codec and the definitions it refers
// to, or the message of the ExportError.
func exportSchema(root string, c codec) (top *Record, defs *Record, refused string) {
	x := &schemaExport{defs: newRecord(0)}
	defer func() {
		if r := recover(); r != nil {
			msg, ok := r.(exportRefused)
			if !ok {
				panic(r)
			}
			top, defs, refused = nil, nil, string(msg)
		}
	}()
	out, ok := x.codec(c, root).(*Record)
	if !ok {
		refuseExport(root, "no value has this type")
	}
	return out, x.defs, ""
}

func exportError(message string) Value { return Err(NewData(exportErrorTag, message)) }

// CodecJsonSchema is `Codec.json_schema root codec`: the schema as JSON
// text, or an ExportError.
func CodecJsonSchema(root, c Value) Value {
	top, defs, refused := exportSchema(root.(string), codecOf(c))
	if refused != "" {
		return exportError(refused)
	}
	out := newRecord(len(top.keys) + 2)
	out.put("$schema", "https://json-schema.org/draft/2020-12/schema")
	for i, k := range top.keys {
		out.put(k, top.vals[i])
	}
	if len(defs.keys) > 0 {
		out.put("$defs", defs)
	}
	return Ok(stringifyIndented(out))
}

// stringifyIndented is `JSON.stringify(v, null, 2)` of a JSON value.
func stringifyIndented(v Value) string {
	var b strings.Builder
	writeIndented(&b, v, "\n")
	return b.String()
}

func writeIndented(b *strings.Builder, v Value, line string) {
	inner := line + "  "
	switch x := v.(type) {
	case []Value:
		if len(x) == 0 {
			b.WriteString("[]")
			return
		}
		b.WriteByte('[')
		for i, e := range x {
			if i > 0 {
				b.WriteByte(',')
			}
			b.WriteString(inner)
			writeIndented(b, e, inner)
		}
		b.WriteString(line + "]")
	case *Record:
		if len(x.keys) == 0 {
			b.WriteString("{}")
			return
		}
		b.WriteByte('{')
		for i, k := range x.keys {
			if i > 0 {
				b.WriteByte(',')
			}
			b.WriteString(inner)
			quoteTo(b, k)
			b.WriteString(": ")
			writeIndented(b, x.vals[i], inner)
		}
		b.WriteString(line + "}")
	default:
		stringify(b, v)
	}
}
