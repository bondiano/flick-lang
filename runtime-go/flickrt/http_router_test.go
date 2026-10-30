package flickrt

import (
	"reflect"
	"testing"
)

// A static segment wins over a parameter, a parameter over a wildcard,
// whatever the order of the routes; the defaults ignore trailing slashes
// and case.
func TestRouterPriority(t *testing.T) {
	routes := []Value{
		newRoute("", "/files/*", "", false, "wild"),
		newRoute("", "/files/:id", "", false, "param"),
		newRoute("GET", "/files/new", "", false, "static"),
	}
	c := configOf(Rec("ignoreTrailingSlash", None, "ignoreDuplicateSlashes", None, "caseSensitive", None, "maxParamLength", None))
	for url, want := range map[string]string{
		"/files/new": "static", "/Files/NEW/": "static", "/files/7": "param", "/files/7/x": "wild",
	} {
		got, _ := find(c, routes, "GET", c.pathSegments(url))
		if got == nil || got.handler != want {
			t.Errorf("%s: got %v, want %s", url, got, want)
		}
	}
	if got, _ := find(c, routes, "POST", c.pathSegments("/files/new")); got == nil || got.handler != "param" {
		t.Errorf("POST /files/new: got %v, want param", got)
	}
	if n := negotiate("br;q=1, gzip;q=0.5", []string{"br", "gzip"}); n != "gzip" {
		t.Errorf("negotiate: %q", n)
	}
}

func TestRouterConstrainedParameters(t *testing.T) {
	c := configOf(Rec("ignoreTrailingSlash", None, "ignoreDuplicateSlashes", None, "caseSensitive", None, "maxParamLength", None))
	for _, tt := range []struct {
		pattern, path string
		want          []Value
	}{
		{"/x/:id([0-9]+)", "/x/abc", nil},
		{"/x/:id([0-9]+)", "/x/123", []Value{[]Value{"id", "123"}}},
		{"/x/:id([0-9]+)", "/x/123x", nil},
		{"/x/:id([0-9]+)", "/x/12%33", []Value{[]Value{"id", "123"}}},
		{"/x/:id([^/]+)", "/x/hello", []Value{[]Value{"id", "hello"}}},
		{"/x/:id([^/]+)", "/x/hello%2Fthere", nil},
		{"/:id([0-9]+)/*", "/", nil},
		{"/:id([0-9]+)/*", "/abc/secret", nil},
		{"/:id([0-9]+)/*", "/123/secret", []Value{[]Value{"id", "123"}, []Value{"*", "secret"}}},
		{"/:a-:b/*", "/ab/secret", nil},
		{"/:a-:b/*", "/a-b/rest", []Value{[]Value{"a", "a"}, []Value{"b", "b"}, []Value{"*", "rest"}}},
		{"/x/:a-:b", "/x/ab", nil},
		{"/x/:a-:b", "/x/a-b-c", []Value{[]Value{"a", "a-b"}, []Value{"b", "c"}}},
		{"/x/:a-:b", "/x/a--", []Value{[]Value{"a", "a"}, []Value{"b", "-"}}},
		{"/x/:a-foo-:b", "/x/a-foo-b-foo-c", []Value{[]Value{"a", "a-foo-b"}, []Value{"b", "c"}}},
		{"/x/:a-:b-:c", "/x/a--", []Value{[]Value{"a", "a"}, []Value{"b", ""}, []Value{"c", ""}}},
		{"/x/pre:a-:b", "/x/prea%0Ab-c", nil},
		{"/x/pre:a-:b", "/x/prea%0Db-c", nil},
		{"/x/pre:a-:b", "/x/prea%E2%80%A8b-c", nil},
		{"/x/pre:a-:b", "/x/prea%E2%80%A9b-c", nil},
		{"/x/Pre:name.json", "/x/prefile.json", []Value{[]Value{"name", "file"}}},
		{"/x/pre:name", "/x/prea%0Ab", []Value{[]Value{"name", "a\nb"}}},
		{"/x/:name.json", "/x/file.txt", nil},
		{"/x/::literal/:name", "/x/:literal/A%20B", []Value{[]Value{"name", "A B"}}},
	} {
		t.Run(tt.pattern+tt.path, func(t *testing.T) {
			got, params := find(c, []Value{newRoute("GET", tt.pattern, "", false, "handler")}, "GET", c.pathSegments(tt.path))
			if (got != nil) != (tt.want != nil) || !reflect.DeepEqual(params, tt.want) && tt.want != nil {
				t.Fatalf("route = %v, params = %v; want params %v", got, params, tt.want)
			}
		})
	}
}

func TestRouterParameterLimitsAndFallback(t *testing.T) {
	c := configOf(Rec("ignoreTrailingSlash", None, "ignoreDuplicateSlashes", None, "caseSensitive", None, "maxParamLength", Some(int32(2))))
	routes := []Value{
		newRoute("GET", "/x/:id", "", false, "plain"),
		newRoute("GET", "/x/:id([0-9]+)", "", false, "number"),
		newRoute("GET", "/x/:a-:b", "", false, "pair"),
		newRoute("GET", "/x/*", "", false, "wild"),
	}
	for _, tt := range []struct{ path, handler string }{
		{"/x/12", "number"},
		{"/x/ab", "plain"},
		{"/x/ab-cd", "pair"},
		{"/x/abc-d", "wild"},
		{"/x/123", "wild"},
		{"/x/😀", "plain"},
	} {
		got, _ := find(c, routes, "GET", c.pathSegments(tt.path))
		if got == nil || got.handler != tt.handler {
			t.Fatalf("%s: got %v, want %s", tt.path, got, tt.handler)
		}
	}
	c.maxParam = 1
	if got, _ := find(c, routes, "GET", c.pathSegments("/x/😀")); got == nil || got.handler != "wild" {
		t.Fatalf("UTF-16 length exceeded: got %v, want wildcard", got)
	}
}

func TestRouterStaticSuffixPriority(t *testing.T) {
	c := configOf(Rec("ignoreTrailingSlash", None, "ignoreDuplicateSlashes", None, "caseSensitive", None, "maxParamLength", None))
	routes := []Value{
		newRoute("GET", "/x/:name", "", false, "plain"),
		newRoute("GET", "/x/:name.json", "", false, "json"),
		newRoute("GET", "/x/:name.foo.json", "", false, "foo"),
		newRoute("GET", "/x/pre:name.json", "", false, "prefix"),
	}
	for _, tt := range []struct{ path, handler, name string }{
		{"/x/file.foo.json", "foo", "file"},
		{"/x/file.json", "json", "file"},
		{"/x/prefile.json", "prefix", "file"},
	} {
		got, params := find(c, routes, "GET", c.pathSegments(tt.path))
		if got == nil || got.handler != tt.handler || !reflect.DeepEqual(params, []Value{[]Value{"name", tt.name}}) {
			t.Fatalf("%s: route %v, params %v; want %s with name %q", tt.path, got, params, tt.handler, tt.name)
		}
	}
}

func TestRouterRejectsInvalidOrUnsupportedRegex(t *testing.T) {
	for _, pattern := range []string{"/x/:id([)", "/x/:id(?=foo)"} {
		_, cause := catch(func() Value { return newRoute("GET", pattern, "", false, Unit) })
		if cause == nil || !cause.has(causeDie) {
			t.Fatalf("%s: got %v, want a route configuration defect", pattern, cause)
		}
	}
}

func TestRouterPrefixedRegexPriority(t *testing.T) {
	c := configOf(Rec("ignoreTrailingSlash", None, "ignoreDuplicateSlashes", None, "caseSensitive", None, "maxParamLength", None))
	routes := []Value{
		newRoute("GET", "/x/pre:name", "", false, "plain"),
		newRoute("GET", "/x/pre:id([0-9]+)", "", false, "number"),
	}
	for _, tt := range []struct{ path, handler, key, value string }{
		{"/x/pre123", "number", "id", "123"},
		{"/x/preabc", "plain", "name", "abc"},
	} {
		got, params := find(c, routes, "GET", c.pathSegments(tt.path))
		if got == nil || got.handler != tt.handler || !reflect.DeepEqual(params, []Value{[]Value{tt.key, tt.value}}) {
			t.Fatalf("%s: got %v with params %v, want %s", tt.path, got, params, tt.handler)
		}
	}
}

func TestRouterDistinctParameterNodeOrder(t *testing.T) {
	c := configOf(Rec("ignoreTrailingSlash", None, "ignoreDuplicateSlashes", None, "caseSensitive", None, "maxParamLength", None))
	routes := []Value{
		newRoute("GET", "/x/:id([0-9]+)/other", "", false, "number-other"),
		newRoute("GET", "/x/:id([a-z0-9]+)/fixed", "", false, "alpha-fixed"),
		newRoute("GET", "/x/:number([0-9]+)/:rest", "", false, "number-rest"),
	}
	if got, _ := find(c, routes, "GET", c.pathSegments("/x/123/fixed")); got == nil || got.handler != "number-rest" {
		t.Fatalf("distinct regex branches: got %v, want number-rest", got)
	}
	if got, _ := find(c, routes, "GET", c.pathSegments("/x/abc/fixed")); got == nil || got.handler != "alpha-fixed" {
		t.Fatalf("nonmatching first regex branch: got %v, want alpha-fixed", got)
	}
	shared := []Value{
		routes[2],
		newRoute("GET", "/x/:n([0-9]+)/fixed", "", false, "number-fixed"),
	}
	for _, tt := range []struct{ path, handler string }{
		{"/x/123/fixed", "number-fixed"},
		{"/x/123/other", "number-rest"},
	} {
		got, _ := find(c, shared, "GET", c.pathSegments(tt.path))
		if got == nil || got.handler != tt.handler {
			t.Fatalf("%s: got %v, want %s", tt.path, got, tt.handler)
		}
	}
}
