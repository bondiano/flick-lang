package flickrt

import (
	"bytes"
	"encoding/hex"
	"slices"
	"strings"
	"testing"
)

// The layouts of binVectors (rpc_binary_vectors_test.go), by the prefix of
// a vector's name: the same codecs the Node generator gave Effect.
func binTestLayouts() map[string]*binNode {
	json := func(s Value) codec { return codecOf(SchemaJsonCodec(s)) }
	layout := func(s Value) *binNode { return binCodecLayout(json(s)) }
	num := json(SchemaNumber)
	record := SchemaRecord("10", SchemaNumber, "2", SchemaString, "x", SchemaBool)
	notFound := SchemaAdt("NotFound", []Variant{{"M.NotFound.NotFound", "NotFound", []Value{SchemaRecord("id", SchemaNumber)}}})
	shape := SchemaAdt("Shape", []Variant{
		{"M.Shape.Circle", "Circle", []Value{SchemaNumber}},
		{"M.Shape.Rect", "Rect", []Value{SchemaNumber, SchemaNumber}},
		{"M.Shape.Empty", "Empty", nil},
	})
	var intList Value
	intListRef := SchemaLazy("IntList", func() Value { return intList })
	intList = SchemaAdt("IntList", []Variant{
		{"M.IntList.Nil", "Nil", nil},
		{"M.IntList.Cons", "Cons", []Value{SchemaInt, intListRef}},
	})
	block := CodecBlock("B", []Value{"a", DslField("a", CodecString), "b", DslOptionalField("bee", SchemaJsonCodec(SchemaInt))})
	metaRow := CodecBlock("Row", []Value{"meta", DslField("meta", CodecIgnoreExtra(block))})
	row := SchemaRecord("name", SchemaString, "n", SchemaNumber, "tag", SchemaString)
	wide := make([]Value, 0, 62)
	for i := range 31 {
		wide = append(wide, "f"+ShowNumber(float64(i)), SchemaNumber)
	}
	unicode := CodecBlock("U", []Value{"e", DslField("é", CodecString), "z", DslField("z", CodecString), "a", DslField("A", CodecString)})
	floats := SchemaRecord("d", SchemaNumber, "f", SchemaNumber, "b", SchemaBool, "s", SchemaString)
	return map[string]*binNode{
		"number":        layout(SchemaNumber),
		"int":           layout(SchemaInt),
		"string":        layout(SchemaString),
		"bool":          layout(SchemaBool),
		"bigint":        layout(SchemaBigint),
		"unit":          layout(SchemaUnit),
		"numlist":       layout(SchemaList(SchemaNumber)),
		"strlist":       layout(SchemaList(SchemaString)),
		"boollist":      layout(SchemaList(SchemaBool)),
		"tuple":         layout(SchemaTuple(SchemaNumber, SchemaString, SchemaBool)),
		"option":        layout(SchemaOption(SchemaInt)),
		"record":        layout(record),
		"optrecord":     layout(SchemaOption(record)),
		"notfound":      layout(notFound),
		"shape":         layout(shape),
		"optshape":      layout(SchemaOption(shape)),
		"intlist":       layout(intListRef),
		"codeclist":     binCodecLayout(codecOf(CodecList(SchemaJsonCodec(SchemaInt)))),
		"tagged":        binCodecLayout(codecOf(CodecTaggedOption(CodecString))),
		"block":         binCodecLayout(codecOf(block)),
		"ignore":        binCodecLayout(codecOf(CodecIgnoreExtra(block))),
		"lossless":      binCodecLayout(codecOf(CodecNumberLossless)),
		"json":          binCodecLayout(codecOf(JsonCodec)),
		"unknown":       layout(SchemaUnknown),
		"rows":          layout(SchemaList(row)),
		"tagrows":       layout(SchemaList(SchemaRecord("name", SchemaString, "tags", SchemaList(SchemaString)))),
		"blockrows":     binCodecLayout(codecOf(CodecList(block))),
		"metarows":      binCodecLayout(codecOf(CodecList(metaRow))),
		"longrows":      layout(SchemaList(SchemaRecord("k", SchemaString))),
		"widerows":      layout(SchemaList(SchemaRecord(wide...))),
		"unicode":       binCodecLayout(codecOf(unicode)),
		"unicodeignore": binCodecLayout(codecOf(CodecIgnoreExtra(unicode))),
		"floats":        layout(floats),
		"optopt":        layout(SchemaOption(SchemaOption(SchemaInt))),
		"unitlist":      layout(SchemaList(SchemaUnit)),
		"unittuple":     layout(SchemaTuple(SchemaUnit, SchemaNumber)),
		"optlist":       layout(SchemaList(SchemaOption(SchemaString))),
		"rowsexit":      binExitLayout(json(SchemaList(row)), nil, false),
		"exit":          binExitLayout(num, []codec{json(notFound)}, false),
		"voidexit":      binExitLayout(nil, nil, false),
		"streamexit":    binExitLayout(num, []codec{json(notFound)}, true),
		"multiexit":     binExitLayout(num, []codec{json(notFound), json(shape), codecOf(CodecString)}, false),
		"unknownexit":   binUnknownExitLayout,
		"defect":        binDefectLayout,
		"chunk":         binChunkLayout(num),
		"rowchunk":      binChunkLayout(json(row)),
		"strchunk":      binChunkLayout(json(SchemaString)),
		"payload":       binPayloadLayout(nil),
	}
}

var binHoleKeys = []string{"payload", "values", "exit", "defect"}

// binMessage is a vector's message with its holes, written in hex, as bytes.
func binMessage(t *testing.T, m Value) Value {
	r := m.(*Record)
	out := newRecord(len(r.keys))
	for i, k := range r.keys {
		v := r.vals[i]
		if s, ok := v.(string); ok && slices.Contains(binHoleKeys, k) {
			b, err := hex.DecodeString(s)
			if err != nil {
				t.Fatal(err)
			}
			v = b
		}
		out.put(k, v)
	}
	return out
}

// binShow is a message's JSON text, its holes in hex.
func binShow(v Value) string {
	if r, ok := v.(*Record); ok {
		out := newRecord(len(r.keys))
		for i, k := range r.keys {
			x := r.vals[i]
			if b, ok := x.([]byte); ok {
				x = hex.EncodeToString(b)
			}
			out.put(k, x)
		}
		v = out
	}
	return JsonStringify(v).(string)
}

func binTree(t *testing.T, text string) Value {
	v, reason := parseJSON(text)
	if reason != "" {
		t.Fatal(reason)
	}
	return v
}

func binSorted(v Value) Value {
	r, ok := v.(*Record)
	if !ok {
		return v
	}
	out := newRecord(len(r.keys))
	for _, k := range sortedKeys(r) {
		out.put(k, r.Get(k))
	}
	return out
}

func TestBinVectors(t *testing.T) {
	layouts := binTestLayouts()
	for _, v := range binVectors {
		prefix, _, _ := strings.Cut(v.name, "/")
		want, _ := hex.DecodeString(v.hex)
		tree := binTree(t, v.tree)
		switch prefix {
		case "msgs":
			continue // TestBinParserSplit
		case "msg":
			msg := binMessage(t, tree)
			if got := hex.EncodeToString(binEncodeMessages([]Value{msg})); got != v.hex {
				t.Errorf("%s: encode\n got %s\nwant %s", v.name, got, v.hex)
			}
			out, err := newBinParser().feed(want)
			if err != nil || len(out) != 1 || binShow(out[0]) != v.tree {
				t.Errorf("%s: decode %v %v", v.name, out, err)
			}
			continue
		}
		n := layouts[prefix]
		if n == nil {
			t.Fatalf("%s: no layout", v.name)
		}
		got, err := binEncode(n, tree)
		if err != nil || hex.EncodeToString(got) != v.hex {
			t.Errorf("%s: encode %v\n got %x\nwant %s", v.name, err, got, v.hex)
		}
		back, err := binDecode(n, want)
		// An ignore-extra block's keys come back in wire order, sorted.
		if strings.HasSuffix(prefix, "ignore") {
			back, tree = binSorted(back), binSorted(tree)
		}
		if err != nil || JsonStringify(back) != JsonStringify(tree) {
			t.Errorf("%s: decode %v\n got %v\nwant %s", v.name, err, JsonStringify(back), v.tree)
		}
	}
}

// The parser reads the frames JS wrote however the bytes are split.
func TestBinParserSplit(t *testing.T) {
	var many struct{ tree, hex string }
	for _, v := range binVectors {
		if v.name == "msgs/many" {
			many = struct{ tree, hex string }{v.tree, v.hex}
		}
	}
	wire, _ := hex.DecodeString(many.hex)
	var msgs []Value
	for _, m := range binTree(t, many.tree).([]Value) {
		msgs = append(msgs, binMessage(t, m))
	}
	if !bytes.Equal(binEncodeMessages(msgs), wire) {
		t.Fatalf("encodeMany\n got %x\nwant %x", binEncodeMessages(msgs), wire)
	}
	for _, step := range []int{1, 2, 7, 64, len(wire)} {
		p := newBinParser()
		var got []string
		for at := 0; at < len(wire); at += step {
			out, err := p.feed(wire[at:min(at+step, len(wire))])
			if err != nil {
				t.Fatalf("step %d: %v", step, err)
			}
			for _, m := range out {
				got = append(got, binShow(m))
			}
		}
		if len(got) != len(msgs) || len(p.buf) != 0 {
			t.Fatalf("step %d: %d messages, %d bytes left", step, len(got), len(p.buf))
		}
		for i, m := range msgs {
			if got[i] != binShow(m) {
				t.Errorf("step %d: message %d\n got %s\nwant %s", step, i, got[i], binShow(m))
			}
		}
	}
}

// A bad frame fails after the messages before it, and the parser stays failed.
func TestBinParserError(t *testing.T) {
	ping := binEncodeMessages([]Value{Rec("_tag", "Ping")})
	bad := append(append([]byte{}, ping...), 0x02, 0x21, 0x00)
	p := newBinParser()
	out, err := p.feed(bad)
	if len(out) != 1 || err == nil {
		t.Fatalf("got %v, %v", out, err)
	}
	if out, err := p.feed(ping); out != nil || err == nil {
		t.Fatalf("after an error: %v, %v", out, err)
	}
}

// The actual Effect schema for Result is an untagged union of two structs.
// It has no uniquely identifiable binary layout: a Go-only JSON fallback
// would emit bytes that an Effect peer cannot decode.
func TestBinResultRejectsAmbiguousLayout(t *testing.T) {
	n := binCodecLayout(codecOf(SchemaJsonCodec(SchemaResult(SchemaString, SchemaNumber))))
	for _, text := range []string{`{"ok":1.5}`, `{"err":"no"}`} {
		if _, err := binEncode(n, binTree(t, text)); err == nil || !strings.Contains(err.Error(), "union members are not uniquely identifiable") {
			t.Fatalf("%s: expected Effect's ambiguous-layout rejection, got %v", text, err)
		}
	}
}

// Values the layout does not describe fail to encode instead of writing
// bytes a peer would misread.
func TestBinEncodeRefuses(t *testing.T) {
	layouts := binTestLayouts()
	for prefix, text := range map[string]string{
		"number":   `"1"`,
		"notfound": `{"_tag":"Other"}`,
		"block":    `{"bee":1}`,
		"tuple":    `[1,"x"]`,
		"exit":     `{"_tag":"Failure","cause":[{"_tag":"Fail","error":"x"}]}`,
		"voidexit": `{"_tag":"Failure","cause":[{"_tag":"Fail","error":null}]}`,
	} {
		if _, err := binEncode(layouts[prefix], binTree(t, text)); err == nil {
			t.Errorf("%s: %s encoded", prefix, text)
		}
	}
}
