package flickrt

import "testing"

func roundTrip(t *testing.T, c Value, in Value) Value {
	t.Helper()
	fb := rootFiber()
	v, issues := codecOf(c).decode(fb, in)
	if issues != nil {
		t.Fatalf("%v gave %v", in, issues)
	}
	w, issues := codecOf(c).encode(fb, v)
	if issues != nil || JsonStringify(w) != JsonStringify(in) {
		t.Fatalf("%v came back as %v %v", in, w, issues)
	}
	return v
}

// Effect's key-value splitting drops pairs without a key or a value.
func TestSplitKeyValue(t *testing.T) {
	v, _ := codecOf(CodecSplitKeyValue("&", "=")).decode(rootFiber(), "a=1&=2&b=&c=3=4&a=5")
	if got := JsonStringify(v); got != `[["a","5"],["c","3=4"]]` {
		t.Fatalf("split %s", got)
	}
}

func TestTextBytes(t *testing.T) {
	if v := roundTrip(t, CodecStringFromBase64Url, "w6k"); v != "é" {
		t.Fatalf("base64url gave %v", v)
	}
	if _, issues := codecOf(CodecStringFromBase64).decode(rootFiber(), "aGk"); issues == nil {
		t.Fatal("unpadded base64 decoded")
	}
	roundTrip(t, CodecStringFromHex, "c3a9")
}

func TestListEnsure(t *testing.T) {
	c := CodecListEnsure(CodecString)
	if v := roundTrip(t, c, "a"); len(List(v)) != 1 {
		t.Fatalf("one element gave %v", v)
	}
	roundTrip(t, c, []Value{"a", "b"})
	if _, issues := codecOf(c).decode(rootFiber(), 1.0); len(issues) != 1 || issues[0].message != "expected Union, got number" {
		t.Fatalf("a number gave %v", issues)
	}
}

// encode_xml writes Effect's pretty XML: keys sorted, unsafe names renamed.
func TestEncodeXml(t *testing.T) {
	got := CodecEncodeXml(rootFiber(), JsonCodec, Rec("b", []Value{1.0, true}, "a b", JsonNull, "c", "<&>"))
	want := "<root>\n  <a_b data-name=\"a b\"/>\n  <b>\n    <item>1</item>\n    <item>true</item>\n  </b>\n  <c>&lt;&amp;&gt;</c>\n</root>"
	if got != want {
		t.Fatalf("xml %q", got)
	}
}
