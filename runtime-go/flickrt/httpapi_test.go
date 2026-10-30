package flickrt

import "testing"

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
