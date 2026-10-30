package flickrt

import (
	"fmt"
	"math"
	"strings"
	"testing"
)

// The request of tools/load/json, as its generated handler runs it: parse the
// stored document, decode it, update the record and answer with its JSON.
var (
	loadPart Value
	loadItem = SchemaJsonCodec(SchemaRecord(
		"id", SchemaString, "label", SchemaString, "note", SchemaOption(SchemaString),
		"parts", SchemaList(SchemaLazy("Part", func() Value { return loadPart })),
		"price", SchemaNumber, "tags", SchemaList(SchemaString)))
	loadDefaults = Rec("status", None, "statusText", None, "headers", None, "cookies", None, "contentType", None)
	loadStored   = `{"id": "", "label": "", "price": 12.5, "tags": ["steel", "blue", "large"], "parts": [{"_tag": "Bolt", "_0": 3}, {"_tag": "Panel", "_0": 2, "_1": 4.5}, {"_tag": "Bolt", "_0": 8}], "note": null}`
)

func init() {
	loadPart = SchemaAdt("Part", []Variant{
		{"Bolt#f3ef338b8947", "Bolt", []Value{SchemaNumber}},
		{"Panel#24aca5ca3513", "Panel", []Value{SchemaNumber, SchemaNumber}},
	})
}

func loadRequest(fb *Fiber, id string) *ServerResponse {
	base := FromResult(CodecDecodeResult(fb, loadItem, Get(JsonParse(loadStored), 0)))
	made := Update(base, Rec("id", id, "label", Concat("item ", id), "price", MulF(Field(base, "price"), 2.0)))
	return ResponseSchemaJson(fb, loadItem, made, loadDefaults).(*ServerResponse)
}

func TestLoadRequest(t *testing.T) {
	want := `{"id":"7","label":"item 7","note":null,"parts":[{"_tag":"Bolt","_0":3},{"_tag":"Panel","_0":2,"_1":4.5},{"_tag":"Bolt","_0":8}],"price":25,"tags":["steel","blue","large"]}`
	if got := loadRequest(nil, "7").body.data; got != want {
		t.Fatalf("body\n got %s\nwant %s", got, want)
	}
}

func BenchmarkLoadRequest(b *testing.B) {
	b.ReportAllocs()
	for range b.N {
		loadRequest(nil, "7")
	}
}

// write is stringify of encode, issues included.
func TestWriteIsEncoded(t *testing.T) {
	base := FromResult(CodecDecodeResult(nil, loadItem, Get(JsonParse(loadStored), 0)))
	w := loadItem.(*wire)
	for _, v := range []Value{
		base,
		Update(base, Rec("note", Some("a \"q\"\n\xed\xa0\x80"), "price", -0.0)),
		Update(base, Rec("price", math.NaN())),
	} {
		var got, want strings.Builder
		gotIssues, wantIssues := w.write(nil, &got, v), w.writeEncoded(nil, &want, v)
		if fmt.Sprint(gotIssues) != fmt.Sprint(wantIssues) || wantIssues == nil && got.String() != want.String() {
			t.Errorf("write %v %s, encode %v %s", gotIssues, got.String(), wantIssues, want.String())
		}
	}
}
