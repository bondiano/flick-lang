package flickrt

import (
	"strings"
	"sync"
	"testing"
)

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

func TestURLShowUsesExecutingFiber(t *testing.T) {
	const raw = "https://user:password@example.com/?private_key=secret&other_key=visible&sig=signed"
	u := value(UrlFromString(raw))
	show := Fn(UrlShow)
	showLater := Fn(func(fb *Fiber, v Value) Value { return Apply(fb, show, v) })
	urlDict := NewDict().Set("show", show)
	pairDict := Structural("Show", []Value{urlDict, urlDict})
	wrapped := Derive("Show", func(*Fiber) []Value { return []Value{urlDict} },
		[]Constructor{{Tag: "Wrapped", Name: "Wrapped", Arity: 1}}, nil)
	shown := func(query string) string { return "https://REDACTED:REDACTED@example.com/?" + query }
	defaults := shown("private_key=secret&other_key=visible&sig=REDACTED")
	private := shown("private_key=REDACTED&other_key=visible&sig=signed")
	other := shown("private_key=secret&other_key=REDACTED&sig=signed")
	none := shown("private_key=secret&other_key=visible&sig=signed")
	root := rootFiber()
	if got := UrlShow(nil, u); got != defaults {
		t.Fatalf("outside a fiber: %v", got)
	}
	UrlParamsWithRedactedNames(root, []Value{"PRIVATE_KEY"}, Eff(func(fb *Fiber) Value {
		for name, got := range map[string]Value{
			"direct":   UrlShow(fb, u),
			"closure":  Apply(fb, showLater, u),
			"list map": At(ListMap(fb, show, []Value{u}), 0),
		} {
			if got != private {
				t.Errorf("%s: got %v, want %s", name, got, private)
			}
		}
		if got := Apply(fb, Method(fb, pairDict, "show"), []Value{u, u}); got != "("+private+", "+private+")" {
			t.Errorf("structural Show: %v", got)
		}
		UrlParamsWithRedactedNames(fb, []Value{"other_key"}, Eff(func(inner *Fiber) Value {
			if got := UrlShow(inner, u); got != other {
				t.Errorf("nested names replace outer names: %v", got)
			}
			return Unit
		}))
		UrlParamsWithRedactedNames(fb, []Value{}, Eff(func(inner *Fiber) Value {
			if got := UrlShow(inner, u); got != none {
				t.Errorf("empty names still hide userinfo: %v", got)
			}
			return Unit
		}))
		if got := UrlShow(fb, u); got != private {
			t.Errorf("nested scope changed outer names: %v", got)
		}
		return Unit
	}))
	if got := UrlShow(root, u); got != defaults || UrlHref(u) != raw {
		t.Fatalf("scope changed defaults or URL: %v / %v", got, UrlHref(u))
	}
	ready, release := make(chan struct{}, 2), make(chan struct{})
	var done sync.WaitGroup
	for _, test := range []struct{ name, want string }{{"private_key", private}, {"other_key", other}} {
		done.Add(1)
		go func() {
			defer done.Done()
			UrlParamsWithRedactedNames(rootFiber(), []Value{test.name}, Eff(func(fb *Fiber) Value {
				ready <- struct{}{}
				<-release
				for range 20 {
					if got := Apply(fb, showLater, u); got != test.want {
						t.Errorf("concurrent closure (%s): %v", test.name, got)
					}
					if got := Apply(fb, Method(fb, wrapped, "show"), NewData("Wrapped", u)); got != "Wrapped "+test.want {
						t.Errorf("concurrent derived Show (%s): %v", test.name, got)
					}
				}
				return Unit
			}))
		}()
	}
	<-ready
	<-ready
	close(release)
	done.Wait()
}

func TestHTTPFailuresUseCurrentURLRedaction(t *testing.T) {
	const raw = "http://example.com/?private_key=secret&sig=signed"
	request := ClientRequestGet(raw)
	badResponse := ResponseToClientResponse(request, ResponseText("not json", Update(noOptions, Rec("status", Some(int32(400))))))
	badRequest := RequestFromClientRequest(ClientRequestBodyText("not json", Some("application/json"), ClientRequestPost(raw)))
	client := clientOfTransport(fetchTransport)
	tests := []struct {
		name, reason string
		run          func(*Fiber) Value
	}{
		{"invalid URL", invalidUrlTag, func(fb *Fiber) Value {
			return HttpClientExecute(fb, client, ClientRequestGet("http://[?private_key=secret&sig=signed"))
		}},
		{"transport", transportErrorTag, func(fb *Fiber) Value {
			return HttpClientExecute(fb, client, ClientRequestGet("unsupported://example.com/?private_key=secret&sig=signed"))
		}},
		{"status", statusErrorTag, func(fb *Fiber) Value { return ClientResponseFilterStatusOk(fb, badResponse) }},
		{"decode", decodeErrorTag, func(fb *Fiber) Value { return ClientResponseJson(fb, badResponse) }},
		{"server parse", parseErrorTag, func(fb *Fiber) Value { return RequestJson(fb, badRequest) }},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var failure Value
			UrlParamsWithRedactedNames(rootFiber(), []Value{"private_key"}, Eff(func(fb *Fiber) Value {
				_, cause := catch(func() Value { return test.run(fb) })
				if cause == nil || cause.firstOf(causeFail) == nil {
					t.Fatalf("operation did not produce a typed failure: %v", cause)
				}
				failure = cause.firstOf(causeFail).err
				return Unit
			}))
			reason := Get(failure, 0).(*Data)
			if reason.Tag != test.reason {
				t.Fatalf("reason: %s, want %s", reason.Tag, test.reason)
			}
			url := Field(Get(reason, 0), "url").(string)
			if !strings.Contains(url, "private_key=REDACTED") || strings.Contains(url, "private_key=secret") || !strings.Contains(url, "sig=signed") {
				t.Errorf("error escaped its scope with wrong redaction: %s", url)
			}
		})
	}
	if ClientRequestUrl(request) != raw {
		t.Error("redaction changed the request URL")
	}
}
