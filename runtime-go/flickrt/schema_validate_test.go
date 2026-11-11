package flickrt

import "testing"

// Schema.validate checks the decoded side of a schema.
func TestSchemaValidate(t *testing.T) {
	fb := rootFiber()
	rec := SchemaRecord("n", SchemaInt, "s", SchemaList(SchemaString))
	v := SchemaValidate(fb, rec, Rec("n", 2.0, "s", []Value{"a"}))
	if !isOk(v) || record(value(v)).Get("n") != int32(2) {
		t.Fatalf("a record gave %v", v)
	}
	bad := SchemaValidate(fb, rec, Rec("n", 2.0, "s", []Value{1.0}))
	if isOk(bad) || JsonStringify(List(value(bad).(*Data).Fields()[0])[0]) != `{"message":"expected a string, got number","path":"s[0]"}` {
		t.Fatalf("a wrong element gave %v", bad)
	}
}
