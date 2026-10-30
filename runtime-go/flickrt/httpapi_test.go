package flickrt

import (
	"net/http"
	"testing"
)

func TestApiURL(t *testing.T) {
	e := &apiEndpoint{path: "/users/:id/x"}
	cases := map[string]string{
		"http://h.test":          "http://h.test/users/a%20b/x?q=1",
		"http://h.test/api":      "http://h.test/api/users/a%20b/x?q=1",
		"http://h.test/base?x#f": "http://h.test/users/a%20b/x?q=1",
		"http://h.test/a/b/?x=1": "http://h.test/a/b/users/a%20b/x?q=1",
	}
	for base, want := range cases {
		got := apiURL(base, e, []Value{[]Value{"id", "a b"}}, []Value{[]Value{"q", "1"}})
		if got != want {
			t.Errorf("%s: got %s, want %s", base, got, want)
		}
	}
}

func TestSseEvents(t *testing.T) {
	events := sseEvents("id: 7\r\nevent: note\ndata: two\ndata:lines\n\n: comment\n\ndata: x\n\n")
	if len(events) != 2 {
		t.Fatalf("got %d events", len(events))
	}
	first := record(events[0])
	if v, _ := some(first.Get("id")); v != "7" || first.Get("event") != "note" || first.Get("data") != "two\nlines" {
		t.Errorf("first event %v", first)
	}
	if second := record(events[1]); second.Get("event") != "message" || second.Get("data") != "x" {
		t.Errorf("second event %v", second)
	}
}

func TestStringifyIndented(t *testing.T) {
	got := stringifyIndented(Rec("a", []Value{}, "b", Rec(), "c", []Value{float64(1), false}))
	want := "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    1,\n    false\n  ]\n}"
	if got != want {
		t.Errorf("got %q", got)
	}
}

func TestApiCodecsUseExecutingFiber(t *testing.T) {
	var want *Fiber
	calls := 0
	identity := Fn(func(fb *Fiber, v Value) Value {
		if fb != want {
			t.Errorf("callback fiber %p, want %p", fb, want)
		}
		calls++
		return v
	})
	c := codecOf(CodecTransformPure(identity, identity, SchemaJsonCodec(SchemaUnknown)))
	endpoint := &apiEndpoint{
		path: "/users/:id", params: c, query: c, success: c, status: 200, successEncoding: encJSON,
		errors: apiErrors{{status: 400, codec: c}},
	}
	withHeaders := *endpoint
	withHeaders.extras = []apiExtra{{headers: c}}
	received := &received{status: 200, headers: http.Header{"X-Test": []string{"x"}}}
	tests := []struct {
		name string
		run  func(*testing.T, *Fiber) Value
		want Value
	}{
		{"url", func(t *testing.T, fb *Fiber) Value {
			return FromResult(HttpApiUrl(fb, "http://h.test", endpoint, Rec("id", "x"), Rec()))
		}, "http://h.test/users/x"},
		{"body", func(t *testing.T, fb *Fiber) Value { return bodyOf(BodyJsonSchema(fb, c, "x")).data }, `"x"`},
		{"server decode", func(t *testing.T, fb *Fiber) Value { return apiDecode(fb, c, "x") }, "x"},
		{"client encode", func(t *testing.T, fb *Fiber) Value {
			v, err := clientEncode(fb, c, "x", "payload")
			if err != nil {
				t.Fatalf("encode failed: %v", err)
			}
			return v
		}, "x"},
		{"client success", func(t *testing.T, fb *Fiber) Value { return clientSuccess(fb, endpoint, received, `"x"`) }, "x"},
		{"client error", func(t *testing.T, fb *Fiber) Value {
			_, cause := catch(func() Value { return clientDeclared(fb, endpoint, 400, "x") })
			if cause == nil {
				t.Fatal("declared error did not fail")
			}
			err, ok := typedFailure(cause)
			if !ok {
				t.Fatalf("not a typed failure: %v", cause)
			}
			return err
		}, "x"},
		{"server success", func(t *testing.T, fb *Fiber) Value {
			return responseOf(apiSucceeded(fb, endpoint, "x")).body.data
		}, `"x"`},
		{"server error", func(t *testing.T, fb *Fiber) Value {
			response := responseOf(apiFailed(fb, endpoint, "x"))
			if response.status != 400 {
				t.Errorf("error status %d, want 400", response.status)
			}
			return response.body.data
		}, `"x"`},
		{"client headers", func(t *testing.T, fb *Fiber) Value {
			return record(List(clientExtras(fb, &withHeaders, received, "value"))[1]).Get("x-test")
		}, "x"},
		{"server headers", func(t *testing.T, fb *Fiber) Value {
			response := responseOf(apiSucceeded(fb, &withHeaders, []Value{"value", Rec("x-test", "x")}))
			return response.headers.Get("x-test")
		}, "x"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			for _, fb := range []*Fiber{rootFiber(), rootFiber()} {
				want = fb
				before := calls
				if got := test.run(t, fb); got != test.want {
					t.Errorf("got %v, want %v", got, test.want)
				}
				if calls == before {
					t.Fatal("operation did not invoke its codec callback")
				}
			}
		})
	}
}

func TestApiSavedStreamCodecUsesExecutingFiber(t *testing.T) {
	var want *Fiber
	calls := 0
	identity := Fn(func(fb *Fiber, v Value) Value {
		if fb == nil || fb.state != want.state {
			t.Errorf("stream callback did not run in the consuming fiber")
		}
		calls++
		return v
	})
	c := codecOf(CodecTransformPure(identity, identity, CodecString))
	endpoint := &apiEndpoint{status: 200, successEncoding: encSSE, success: c}
	response := responseOf(apiOtherSuccess(endpoint, StreamFromList([]Value{"x"})))
	for _, fb := range []*Fiber{rootFiber(), rootFiber()} {
		want = fb
		before := calls
		body, contentType, err := response.body.bytes(fb)
		if err != nil {
			t.Fatal(err)
		}
		if string(body) != "data: \"x\"\n\n" || contentType != "text/event-stream" {
			t.Errorf("body %q, content type %q", body, contentType)
		}
		if calls != before+1 {
			t.Errorf("encoded %d elements, want 1", calls-before)
		}
	}
}
