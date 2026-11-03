package flickrt

import "testing"

// The expected texts are Effect 4.1.2's output for the same inputs.

func parsedJSON(t *testing.T, text string) Value {
	t.Helper()
	r := JsonParse(text)
	if !isOk(r) {
		t.Fatalf("not JSON: %s", text)
	}
	return value(r)
}

// effectDocument is a Flick document as Effect's, `root` named `schema`.
func effectDocument(d Value) string {
	r := record(d)
	return JsonStringify(Rec("dialect", r.Get("dialect"), "schema", r.Get("root"), "definitions", r.Get("definitions"))).(string)
}

func TestJsonPatchGetAndApply(t *testing.T) {
	ops := jsonPatchGet(parsedJSON(t, `{"b":[1,2,3],"a":{"x/y":1},"c":null}`),
		parsedJSON(t, `{"b":[1],"a":{"x/y":2,"z~":true},"d":0}`), "", nil)
	diff := make([]Value, len(ops))
	for i, op := range ops {
		diff[i] = op.json()
	}
	got := JsonStringify(diff).(string)
	want := `[{"op":"replace","path":"/a/x~1y","value":2},{"op":"add","path":"/a/z~0","value":true},{"op":"remove","path":"/b/2"},{"op":"remove","path":"/b/1"},{"op":"remove","path":"/c"},{"op":"add","path":"/d","value":0}]`
	if got != want {
		t.Errorf("diff: got %s", got)
	}
	for _, c := range []struct{ op, path, doc, want string }{
		{"remove", "/nothing/here", `{"a":1}`, `Cannot remove at "/nothing/here" (parent not found or not a container).`},
		{"add", "/a/-", `{"a":[1]}`, `{"a":[1,9]}`},
		{"replace", "/a/5", `{"a":[1]}`, `Array index out of bounds at "/a/5".`},
		{"remove", "/a/01", `{"a":[1]}`, `Invalid array index: "01"`},
		{"replace", "/b", `{"a":[1]}`, `Property "b" does not exist at "/b".`},
	} {
		out, failure := tryPatch([]patchOp{{c.op, c.path, 9.0}}, parsedJSON(t, c.doc))
		if failure == "" {
			failure = JsonStringify(out).(string)
		}
		if failure != c.want {
			t.Errorf("%s %s: got %s", c.op, c.path, failure)
		}
	}
}

func TestJsonSchemaDialects(t *testing.T) {
	doc := JsonSchemaFromDraft202012(parsedJSON(t, `{"$id":"https://x.test/root.json","type":"object",
		"minimum":1,"exclusiveMinimum":0,"maximum":5,"exclusiveMaximum":3,"const":4,"if":{"type":"string"},
		"then":{"minLength":1},"contains":{"$ref":"#/$defs/n"},"prefixItems":[true,false],
		"items":{"$ref":"other.json#/$defs/k"},"dependentRequired":{"a":[],"b":["c"]},
		"dependentSchemas":{"b":{"$ref":"#/$defs/n"}},"properties":{"y":{"$ref":"#/$defs/n","description":"d"},
		"z":{"$id":"sub/z.json","$defs":{"q":{}},"$ref":"#/$defs/q"}},
		"$defs":{"n":{"type":"number"},"bad key":{"$anchor":"an","type":"string"}}}`))
	for _, c := range []struct {
		name string
		got  Value
		want string
	}{
		{"draft-04", JsonSchemaToDraft04(doc), `{"dialect":"draft-04","schema":{"type":"object","properties":{"y":{"description":"d","allOf":[{"$ref":"#/definitions/n"}]},"z":{"definitions":{"q":{}},"id":"sub/z.json","allOf":[{"$ref":"#/definitions/q"}]}},"items":[{},{"not":{}}],"additionalItems":{"$ref":"other.json#/$defs/k"},"minimum":1,"maximum":3,"exclusiveMaximum":true,"dependencies":{"b":{"allOf":[{"$ref":"#/definitions/n"},{"required":["c"]}]}},"id":"https://x.test/root.json","allOf":[{"anyOf":[{"not":{"type":"string"}},{"minLength":1}]},{"anyOf":[{"not":{"type":"array"}},{"not":{"items":{"not":{"$ref":"#/definitions/n"}}}}]}],"enum":[4]},"definitions":{"n":{"type":"number"},"bad key":{"type":"string","id":"#an"}}}`},
		{"draft-07", JsonSchemaToDraft07(doc), `{"dialect":"draft-07","schema":{"type":"object","minimum":1,"exclusiveMinimum":0,"maximum":5,"exclusiveMaximum":3,"const":4,"if":{"type":"string"},"then":{"minLength":1},"contains":{"$ref":"#/definitions/n"},"properties":{"y":{"description":"d","allOf":[{"$ref":"#/definitions/n"}]},"z":{"definitions":{"q":{}},"$id":"sub/z.json","allOf":[{"$ref":"#/definitions/q"}]}},"items":[true,false],"additionalItems":{"$ref":"other.json#/$defs/k"},"dependencies":{"a":[],"b":{"allOf":[{"$ref":"#/definitions/n"},{"required":["c"]}]}},"$id":"https://x.test/root.json"},"definitions":{"n":{"type":"number"},"bad key":{"type":"string","$id":"#an"}}}`},
		{"from draft-07", JsonSchemaFromDraft07(parsedJSON(t, `{"$id":"http://a.test/s#frag","definitions":{"x":{}},"items":[{"$ref":"#/definitions/x"}],"additionalItems":false,"dependencies":{"a":["b"],"c":{}}}`)), `{"dialect":"draft-2020-12","schema":{"$id":"http://a.test/s","$anchor":"frag","dependentRequired":{"a":["b"]},"dependentSchemas":{"c":{}},"prefixItems":[{"$ref":"#/$defs/x"}],"items":false},"definitions":{"x":{}}}`},
		{"from openapi-3.0", JsonSchemaFromOpenApi30(parsedJSON(t, `{"type":"integer","minimum":1,"exclusiveMinimum":true,"example":3,"properties":{"a":{"$ref":"#/components/schemas/A"}}}`)), `{"dialect":"draft-2020-12","schema":{"type":"integer","examples":[3],"properties":{"a":{"$ref":"#/$defs/A"}},"exclusiveMinimum":1},"definitions":{}}`},
		{"from openapi-3.1", JsonSchemaFromOpenApi31(parsedJSON(t, `{"example":1,"examples":[2],"$schema":"https://spec.openapis.org/oas/3.1/dialect/base","items":{"$ref":"#/components/schemas/B~1c"}}`)), `{"dialect":"draft-2020-12","schema":{"examples":[1,2],"$schema":"https://json-schema.org/draft/2020-12/schema","items":{"$ref":"#/$defs/B~1c"}},"definitions":{}}`},
	} {
		if got := effectDocument(c.got); got != c.want {
			t.Errorf("%s:\n got %s\nwant %s", c.name, got, c.want)
		}
	}
	multi := JsonSchemaToOpenApi31(Rec("dialect", "draft-2020-12",
		"schemas", parsedJSON(t, `[{"$ref":"#/$defs/a b"},{"$ref":"#/$defs/a_b/properties/x"}]`),
		"definitions", parsedJSON(t, `{"a b":{"type":"string"},"a_b":{},"a?b":{}}`)))
	if got := JsonStringify(multi).(string); got != `{"dialect":"openapi-3.1","schemas":[{"$ref":"#/$defs/a b"},{"$ref":"#/components/schemas/a_b/properties/x"}],"definitions":{"a_b_1":{"type":"string"},"a_b":{},"a_b_2":{}}}` {
		t.Errorf("openapi-3.1: got %s", got)
	}
	if formatUriFragment([]string{"a b", "c#"}) != "#/a%20b/c%23" {
		t.Errorf("fragment: %s", formatUriFragment([]string{"a b", "c#"}))
	}
}
