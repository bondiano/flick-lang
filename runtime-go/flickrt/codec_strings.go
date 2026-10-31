package flickrt

import (
	"encoding/base64"
	"encoding/hex"
	"strings"
)

// Codecs of Effect Schema over strings and literals: transformations of a
// JSON string, the text forms of numbers and bytes, and exact values.

// literals is Effect's `Literal` (one value) or `Literals` (a union of
// them, even of one).
type literalCodec struct {
	values []Value
	union  bool
}

func (l *literalCodec) accepts(in Value) bool {
	for _, v := range l.values {
		if in == v {
			return true
		}
	}
	return false
}

func (l *literalCodec) decode(_ *Fiber, in Value) (Value, []issue) {
	switch {
	case l.accepts(in):
		return in, nil
	case l.union:
		return nil, expected("Union", in)
	}
	return nil, expected(JsonStringify(l.values[0]).(string), in)
}

func (l *literalCodec) encode(fb *Fiber, v Value) (Value, []issue) { return l.decode(fb, v) }

func (l *literalCodec) jsonSchema(*schemaExport, string) Value {
	if !l.union {
		return Rec("const", l.values[0])
	}
	return Rec("enum", l.values)
}

func CodecLiteral(v Value) Value   { return &literalCodec{[]Value{v}, false} }
func CodecLiterals(vs Value) Value { return &literalCodec{List(vs), true} }

func identityStage(_ *Fiber, v Value) (Value, []issue) { return v, nil }

// stringStage is a JSON string decoded by `to` and encoded by `back`.
func stringStage(to, back func(Value) Value) codec {
	return &stage{CodecString.(codec), pure(to), pure(back)}
}

// noChange is the encoding of a transformation Effect writes as it is.
func noChange(v Value) Value { return v }

var (
	CodecToLowerCase  Value = stringStage(ToLowerCase, noChange)
	CodecToUpperCase  Value = stringStage(ToUpperCase, noChange)
	CodecCapitalize   Value = stringStage(Capitalize, noChange)
	CodecUncapitalize Value = stringStage(Uncapitalize, noChange)
	CodecSnakeToCamel Value = stringStage(SnakeToCamel, CamelToSnake)

	// CodecTrim is Effect's `Trim`: decoded trimmed, and only a trimmed
	// string is encoded.
	CodecTrim Value = withChecks(&stage{&stringAs{"a string that will be decoded as a trimmed string"}, pure(Trim), identityStage}, isTrimmed())

	// CodecNumberFromString is Effect's: JS `Number` of the text, and the
	// number's JS text back.
	CodecNumberFromString Value = &stage{&stringAs{"a string that will be decoded as a number"}, pure(jsNumber), pure(jsText)}

	CodecFiniteFromString Value = &fallible{withChecks(
		&stage{&stringAs{"a string that will be decoded as a finite number"}, pure(jsNumber), pure(jsText)},
		isFinite(),
	)}

	// CodecBoolFromBit is Effect's `BooleanFromBit`: 0 or 1.
	CodecBoolFromBit Value = &stage{
		&literalCodec{[]Value{float64(0), float64(1)}, true},
		pure(func(v Value) Value { return v == float64(1) }),
		pure(func(v Value) Value { return float64(map[bool]int{false: 0, true: 1}[v.(bool)]) }),
	}

	CodecStringFromBase64 Value = textBytes("a base64 encoded string that will be decoded as a UTF-8 string",
		func(s string) ([]byte, bool) { return decodeBase64(s, base64.StdEncoding) },
		base64.StdEncoding.EncodeToString)
	CodecStringFromBase64Url Value = textBytes("a base64 (URL) encoded string that will be decoded as a UTF-8 string",
		decodeBase64Url, base64.RawURLEncoding.EncodeToString)
	CodecStringFromHex Value = textBytes("a hex encoded string that will be decoded as a UTF-8 string",
		decodeHex, hex.EncodeToString)

	CodecBytesFromBase64 Value = byteCodec("a base64 encoded string that will be decoded as Uint8Array",
		func(s string) ([]byte, bool) { return decodeBase64(s, base64.StdEncoding) },
		base64.StdEncoding.EncodeToString)
	CodecBytesFromBase64Url Value = byteCodec("a base64 (URL) encoded string that will be decoded as a Uint8Array",
		decodeBase64Url, base64.RawURLEncoding.EncodeToString)
	CodecBytesFromHex Value = byteCodec("a hex encoded string that will be decoded as a Uint8Array", decodeHex, hex.EncodeToString)

	CodecStringFromUriComponent Value = &fallible{&stage{
		&stringAs{"a URI component encoded string that will be decoded as a UTF-8 string"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if s, ok := decodeURIComponent(v.(string)); ok {
				return s, nil
			}
			return nil, fail("an invalid value")
		},
		func(_ *Fiber, v Value) (Value, []issue) {
			s, _ := encodeURIComponent(wellFormed(v.(string)))
			return s, nil
		},
	}}

	// CodecChar is Effect's `Char`: one UTF-16 unit.
	CodecChar Value = withChecks(CodecString.(codec), isBetweenLength(1, 1))

	// CodecNatural is Effect's `Natural`: a non-negative safe integer.
	CodecNatural Value = withChecks(numberType, isInt(), isGreaterThanOrEqualTo(0))

	CodecPort Value = withChecks(numberType, isInt(), isBetween(0, 65535, "a port number"))
)

// numberType is Effect's `Number`: any number, NaN and the infinities too.
var numberType codec = &numberAs{"a number"}

type numberAs struct{ expected string }

func (n *numberAs) accepts(in Value) bool { _, ok := number(in); return ok }
func (n *numberAs) decode(_ *Fiber, in Value) (Value, []issue) {
	x, ok := number(in)
	if !ok {
		return nil, expected(n.expected, in)
	}
	return x, nil
}
func (n *numberAs) encode(fb *Fiber, v Value) (Value, []issue) { return n.decode(fb, v) }
func (*numberAs) jsonSchema(*schemaExport, string) Value       { return Rec("type", "number") }

// jsNumber is JS `Number(s)`: blank text is 0, malformed text NaN.
func jsNumber(s Value) Value {
	if n, ok := some(StringToNumber(s)); ok {
		return n
	}
	if Trim(s) == "" {
		return float64(0)
	}
	return NaN
}

// jsText is JS `String(n)` of a number.
func jsText(n Value) Value { return jsString(n) }

// textBytes is a string whose wire form is its UTF-8 bytes written by
// `write` and read by `read`; text that does not read is an invalid value.
func textBytes(expectedText string, read func(string) ([]byte, bool), write func([]byte) string) Value {
	return &fallible{&stage{
		&stringAs{expectedText},
		func(_ *Fiber, v Value) (Value, []issue) {
			b, ok := read(v.(string))
			if !ok {
				return nil, fail("an invalid value")
			}
			return utf8Lossy(b), nil
		},
		pure(func(v Value) Value { return write([]byte(wellFormed(v.(string)))) }),
	}}
}

// byteElem is a byte of Flick's bytes: a whole number from 0 to 255.
var byteElem codec = withChecks(numberType, isInt(), isBetween(0, 255, ""))

// byteCodec is a `Codec.bytes_*`: the bytes of text read by `read`, as a
// list of Ints, written back by `write`.
func byteCodec(expectedText string, read func(string) ([]byte, bool), write func([]byte) string) Value {
	return &fallible{&stage{
		&stringAs{expectedText},
		func(_ *Fiber, v Value) (Value, []issue) {
			b, ok := read(v.(string))
			if !ok {
				return nil, fail("an invalid value")
			}
			out := make([]Value, len(b))
			for i, x := range b {
				out[i] = int32(x)
			}
			return out, nil
		},
		func(fb *Fiber, v Value) (Value, []issue) {
			if _, issues := (&listCodec{byteElem}).encode(fb, v); issues != nil {
				return nil, issues
			}
			return write(bytesOf(v)), nil
		},
	}}
}

// bytesOf is a list of Ints from 0 to 255 as bytes.
func bytesOf(v Value) []byte {
	xs := List(v)
	b := make([]byte, len(xs))
	for i, x := range xs {
		b[i] = byte(numberOf(x))
	}
	return b
}

func decodeHex(s string) ([]byte, bool) { b, err := hex.DecodeString(s); return b, err == nil }

func decodeBase64(s string, enc *base64.Encoding) ([]byte, bool) {
	b, err := enc.DecodeString(strings.NewReplacer("\n", "", "\r", "").Replace(s))
	return b, err == nil
}

// decodeBase64Url is Effect's: padding optional.
func decodeBase64Url(s string) ([]byte, bool) {
	s = strings.NewReplacer("\n", "", "\r", "").Replace(s)
	if len(s)%4 == 1 {
		return nil, false
	}
	return decodeBase64(strings.TrimRight(s, "="), base64.RawURLEncoding)
}

// CodecSplit is `Codec.split`: the empty string is no parts.
func CodecSplit(separator Value) Value {
	return &stage{
		CodecString.(codec),
		pure(func(v Value) Value {
			if v == "" {
				return []Value{}
			}
			return Split(separator, v)
		}),
		pure(func(parts Value) Value { return StringJoin(separator, parts) }),
	}
}

// CodecSplitKeyValue is `Codec.split_key_value`: pairs without a key or a
// value are dropped, and a key given again keeps its first place.
func CodecSplitKeyValue(separator, keyValue Value) Value {
	sep, kv := separator.(string), keyValue.(string)
	return &fallible{&stage{
		CodecString.(codec),
		pure(func(v Value) Value {
			out := newRecord(0)
			for _, pair := range List(Split(sep, v)) {
				key, val, found := strings.Cut(pair.(string), kv)
				if kv == "" {
					parts := List(Split("", pair))
					key, val, found = parts[0].(string), "", len(parts) > 1
					if found {
						val = parts[1].(string)
					}
				}
				if found && key != "" && val != "" {
					out.put(key, val)
				}
			}
			return entriesOf(out)
		}),
		pure(func(v Value) Value {
			parts := []string{}
			for _, e := range List(v) {
				pair := List(e)
				parts = append(parts, pair[0].(string)+kv+pair[1].(string))
			}
			return strings.Join(parts, sep)
		}),
	}}
}

// entriesOf is JS `Object.entries`.
func entriesOf(r *Record) Value {
	out := make([]Value, len(r.keys))
	for i, k := range r.keys {
		out[i] = []Value{k, r.vals[i]}
	}
	return out
}

// CodecJsonString is `Codec.json_string`: JSON text in a string.
func CodecJsonString(c Value) Value {
	inner := codecOf(c)
	return &fallible{&stage{
		&stringAs{"a string that will be decoded as JSON"},
		func(fb *Fiber, v Value) (Value, []issue) {
			parsed, reason := parseJSON(v.(string))
			if reason != "" {
				return nil, fail("an invalid value")
			}
			return inner.decode(fb, parsed)
		},
		func(fb *Fiber, v Value) (Value, []issue) {
			w, issues := inner.encode(fb, v)
			if issues != nil {
				return nil, issues
			}
			return JsonStringify(w), nil
		},
	}}
}
