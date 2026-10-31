package flickrt

import "testing"

// Keywords of checks combine as the JS exporter's: the narrower type, the
// tighter bound, and a clash under allOf.
func TestMergeKeywords(t *testing.T) {
	out := record(Rec("type", "number", "minimum", -5.0, "pattern", "a"))
	out = mergeKeywords(out, record(Rec("type", "integer", "minimum", 0.0, "maximum", 9.0, "pattern", "b")))
	want := `{"type":"integer","minimum":0,"pattern":"a","maximum":9,"allOf":[{"pattern":"b"}]}`
	if got := JsonStringify(out); got != want {
		t.Fatalf("merged %s", got)
	}
}

// A checked codec reports the first failing check, and its export refuses
// a check after a transformation.
func TestCheckedCodec(t *testing.T) {
	fb := rootFiber()
	if _, issues := codecOf(CodecPort).decode(fb, 1.5); len(issues) != 1 || issues[0].message != "expected an integer, got number" {
		t.Fatalf("1.5 gave %v", issues)
	}
	if _, issues := codecOf(CodecTrim).encode(fb, " a"); len(issues) != 1 ||
		issues[0].message != "expected a string with no leading or trailing whitespace, got string" {
		t.Fatalf("encoding an untrimmed string gave %v", issues)
	}
	if r := CodecJsonSchema("T", CodecChar); isOk(r) {
		t.Fatalf("Char, whose length JSON Schema counts otherwise, exported %v", value(r))
	}
}
