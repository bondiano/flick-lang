package flickrt

import "testing"

// The expected texts are what Node's `new URL(input).href` gives.
func TestParseURL(t *testing.T) {
	cases := map[string]string{
		"HTTPS://EX.com:443/./a/../b/%2e%2E/c": "https://ex.com/c",
		"https:\\\\ex.com\\a\\b":               "https://ex.com/a/b",
		"http://0x7f.1/":                       "http://127.0.0.1/",
		"http://[0:0:1:0:0:0:0:0]/":            "http://[0:0:1::]/",
		"http://[::ffff:1.2.3.4]/":             "http://[::ffff:102:304]/",
		"http://münchen.de/straße":             "http://xn--mnchen-3ya.de/stra%C3%9Fe",
		"foo:/.//x":                            "foo:/.//x",
		"mailto:a b":                           "mailto:a b",
		"file:x":                               "file:///x",
		"http://u:@h:080/?q='#f`":              "http://u@h/?q=%27#f%60",
	}
	for in, want := range cases {
		if u, ok := parseURL(in); !ok || u.href() != want {
			t.Errorf("%q: want %q", in, want)
		}
	}
	for _, in := range []string{"nope", "http://u@/", "http://09/", "http://a%2Fb/", "http://h:99999/", "foo://a b/"} {
		if _, ok := parseURL(in); ok {
			t.Errorf("%q parsed", in)
		}
	}
}

// The expected trees are what Effect's makeTreeRecord builds.
func TestTreeRecord(t *testing.T) {
	pairs := []Value{
		[]Value{"user[name]", "a"}, []Value{"tags[]", "x"}, []Value{"tags[]", "y"},
		[]Value{"n", "1"}, []Value{"n", "2"}, []Value{"far[30]", "z"},
	}
	got := JsonStringify(treeRecord(pairs))
	want := `{"user":{"name":"a"},"tags":["x","y"],"n":["1","2"],"far":{"30":"z"}}`
	if got != want {
		t.Errorf("got %s, want %s", got, want)
	}
	if got := formEncode(bracketEntries(treeRecord(pairs))); got != "user%5Bname%5D=a&tags=x&tags=y&n=1&n=2&far%5B30%5D=z" {
		t.Errorf("entries: %s", got)
	}
}
