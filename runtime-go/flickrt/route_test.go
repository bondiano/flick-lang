package flickrt

import "testing"

// A route prints its segments and query and parses them back, checked by
// the type's schema.
func TestRoute(t *testing.T) {
	fb := rootFiber()
	query := SchemaRecord("page", SchemaOption(SchemaNumber), "q", SchemaString)
	schema := SchemaAdt("Page", []Variant{
		{"Home", "Home", nil},
		{"User", "User", []Value{SchemaNumber}},
		{"Search", "Search", []Value{SchemaString, query}},
	})
	codec := RouteCodec(schema, []Route{
		{"Home", "", []Value{}, nil, false},
		{"User", "user", []Value{SchemaNumber}, nil, false},
		{"Search", "search", []Value{SchemaString}, []QueryField{{"page", SchemaNumber, true}, {"q", SchemaString, false}}, false},
	})
	search := NewData("Search", "a/b", Rec("page", Some(2.0), "q", "x y"))
	if got := RoutePrint(codec, search); got != "/search/a%2Fb?page=2&q=x+y" {
		t.Fatalf("printed %q", got)
	}
	for text, want := range map[string]string{
		"/":                        "Home",
		"/user/1e3":                "User",
		"/search/a%2Fb?q=x&page=z": "Search",
		"/user/x":                  "",
		"/search/a":                "",
	} {
		got := RouteParse(fb, codec, text)
		v, ok := some(got)
		if ok != (want != "") || ok && v.(*Data).Tag != want {
			t.Fatalf("%s parsed to %v", text, got)
		}
	}
	page := some2(RouteParse(fb, codec, "/search/a%2Fb?q=x&page=z"))
	if q := record(page.Fields[1]); q.Get("page") != None || q.Get("q") != "x" {
		t.Fatalf("an invalid optional parameter gave %v", Format(q))
	}
}

func some2(v Value) *Data { x, _ := some(v); return x.(*Data) }
