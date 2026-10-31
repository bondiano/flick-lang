package flickrt

import "testing"

// A refined type's witness decodes and encodes its base's form and checks
// the value both ways, as the JS target's filter does.
func TestRefinedSchemaChecksBothWays(t *testing.T) {
	fb := rootFiber()
	email := SchemaRefined("Email", SchemaString, Fn(func(_ *Fiber, v Value) Value { return CheckEmail(v) }), "email")
	json := sch(email).jsonOf()
	if v, issues := json.decode(fb, "a@b.c"); issues != nil || v != "a@b.c" {
		t.Fatalf("decode gave %v %v", v, issues)
	}
	if _, issues := json.decode(fb, "nope"); len(issues) != 1 || issues[0].message != "not a valid Email" {
		t.Fatalf("a bad value gave %v", issues)
	}
	if _, issues := json.encode(fb, "nope"); issues == nil {
		t.Fatal("encode let a bad value through")
	}
	if r := Apply(fb, RefinedParse("Email", Fn(func(_ *Fiber, v Value) Value { return CheckEmail(v) })), "x y@b.c").(*Data); r.Tag != "Failure" {
		t.Fatalf("parse gave %v", r.Tag)
	}
	if got := Format((&schemaExport{defs: &Record{}}).schema(sch(email), "")); got != `{"type":"string","format":"email"}` {
		t.Fatalf("JSON Schema %s", got)
	}
}
