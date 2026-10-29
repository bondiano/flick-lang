package flickrt

import "testing"

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
		got := find(c, routes, "GET", c.pathSegments(url))
		if got == nil || got.handler != want {
			t.Errorf("%s: got %v, want %s", url, got, want)
		}
	}
	if got := find(c, routes, "POST", c.pathSegments("/files/new")); got == nil || got.handler != "param" {
		t.Errorf("POST /files/new: got %v, want param", got)
	}
	if n := negotiate("br;q=1, gzip;q=0.5", []string{"br", "gzip"}); n != "gzip" {
		t.Errorf("negotiate: %q", n)
	}
}
