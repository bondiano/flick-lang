package flickrt

import (
	"math"
	"reflect"
	"testing"
	"time"
)

func promptTestChoices(titles ...string) Value {
	out := make([]Value, len(titles))
	for i, title := range titles {
		out[i] = Rec("title", title, "value", int32(i+1), "description", None, "disabled", false, "selected", false)
	}
	return out
}

func promptTestKeys(script ...any) []Value {
	var keys []Value
	for _, step := range script {
		switch k := step.(type) {
		case string:
			keys = append(keys, promptTestTyped(k)...)
		case Value:
			keys = append(keys, k)
		}
	}
	return keys
}

func TestPromptKindsAnswers(t *testing.T) {
	ret := promptTestPress("return")
	cases := []struct {
		name   string
		prompt Value
		keys   []Value
		want   Value
	}{
		{"int_bounds", PromptInt(Rec("message", "N", "default", None, "min", Some(int32(0)), "max", Some(int32(10)),
			"incrementBy", None, "decrementBy", None, "validate", None)),
			promptTestKeys("42", ret, promptTestPress("backspace"), ret), int32(4)},
		{"int_arrows", PromptInt(Rec("message", "N", "default", Some(int32(5)), "min", None, "max", None,
			"incrementBy", Some(int32(3)), "decrementBy", None, "validate", None)),
			promptTestKeys(promptTestPress("up"), promptTestPress("up"), promptTestPress("down"), ret), int32(10)},
		{"number_precision", PromptNumber(Rec("message", "X", "default", None, "min", None, "max", None,
			"incrementBy", None, "decrementBy", None, "validate", None, "precision", Some(int32(1)))),
			promptTestKeys("-1.25x", ret), -1.2}, // JS Math.round rounds half up,
		{"toggle", PromptToggle(Rec("message", "On", "initial", None, "active", None, "inactive", None)),
			promptTestKeys(promptTestPress("tab"), promptTestPress("tab"), promptTestPress("up"), ret), true},
		{"select_wraps", PromptSelect(Rec("message", None, "choices", promptTestChoices("a", "b", "c"), "maxPerPage", None)),
			promptTestKeys(promptTestPress("up"), ret), int32(3)},
		{"auto_complete_filters", PromptAutoComplete(Rec("message", "L", "choices", promptTestChoices("Go", "Rust", "Ruby"),
			"maxPerPage", None, "filterLabel", None, "filterPlaceholder", None, "emptyMessage", None)),
			promptTestKeys("ru", promptTestPress("down"), ret), int32(3)},
		{"multi_select_bounds", PromptMultiSelect(Rec("message", Some("M"), "choices", promptTestChoices("a", "b", "c"), "maxPerPage", Some(int32(3))),
			Rec("selectAll", None, "selectNone", None, "inverseSelection", None, "min", None, "max", Some(int32(1)))),
			promptTestKeys(promptTestPress("down"), promptTestPress("down"), promptTestPress("space"), promptTestPress("down"), promptTestPress("space"), ret, promptTestPress("space"), ret),
			[]Value{int32(1)}},
		{"all_and_map", PromptAll([]Value{PromptSucceed("x"), PromptMap(Fn(func(_ *Fiber, v Value) Value { return v.(string) + "!" }), promptTestText(None, None))}),
			promptTestKeys("y", ret), []Value{"x", "y!"}},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			script := &promptScript{keys: test.keys}
			if got := promptTestRun(script, test.prompt); !reflect.DeepEqual(got, test.want) {
				t.Fatalf("got %#v, want %#v", got, test.want)
			}
			if len(script.keys) != 0 {
				t.Fatalf("%d keys left", len(script.keys))
			}
		})
	}
}

func TestPromptDateEditsParts(t *testing.T) {
	start := DateTimeMake("2024-01-31T12:00:00Z").(*Data).Fields[0]
	prompt := PromptDate(Rec("message", "When", "initial", Some(start), "dateMask", Some("YYYY-MM-DD"), "validate", None, "locales", None))
	keys := promptTestKeys(promptTestPress("right"), promptTestPress("up"), promptTestPress("x"), promptTestPress("return"))
	got := promptTestRun(&promptScript{keys: keys}, prompt).(*DateTime).t.In(time.Local)
	// as JS `setMonth`, one month after January 31st overflows into March
	if got.Month() != time.March || got.Year() != 2024 {
		t.Fatalf("got %v", got)
	}
	parts := promptDateParts(`DD "at" HH\:mm Do`)
	var kinds []int
	for _, p := range parts {
		kinds = append(kinds, p.kind)
	}
	if want := []int{promptDay, promptToken, promptHours, promptToken, promptMinutes, promptToken, promptDay}; !reflect.DeepEqual(kinds, want) {
		t.Fatalf("parts %v, want %v", parts, want)
	}
	if parts[1].token != " at " || parts[3].token != ":" {
		t.Fatalf("tokens %q %q", parts[1].token, parts[3].token)
	}
}

func TestPromptParseAsJS(t *testing.T) {
	cases := []struct {
		text  string
		float bool
		want  float64
	}{
		{"42abc", false, 42}, {"-7.9", false, -7}, {"x1", false, math.NaN()},
		{"0.", true, 0}, {".5e1x", true, 5}, {"-Infinity", true, math.Inf(-1)}, {"-", true, math.NaN()},
	}
	for _, test := range cases {
		got := promptParse(test.text, test.float)
		if got != test.want && !(math.IsNaN(got) && math.IsNaN(test.want)) {
			t.Errorf("parse %q: got %v, want %v", test.text, got, test.want)
		}
	}
}

func TestPromptCustomEventsWinOverKeys(t *testing.T) {
	events := QueueUnbounded()
	QueueOffer(rootFiber(), "ping", events)
	process := Fn(func(_ *Fiber, input Value) Value {
		return Fn(func(_ *Fiber, _ Value) Value {
			if d := input.(*Data); d.Tag == promptEventTag {
				return NewData(promptSubmitTag, d.Fields[0])
			}
			return Nullary(promptBeepTag)
		})
	})
	draw := Fn(func(*Fiber, Value) Value { return Fn(func(*Fiber, Value) Value { return "" }) })
	handlers := Rec("render", draw, "process", process, "clear", draw)
	script := &promptScript{keys: promptTestKeys("k")}
	if got := promptTestRun(script, PromptCustomEvents(int32(0), events, handlers)); got != "ping" || len(script.keys) != 1 {
		t.Fatalf("got %v with %d keys left", got, len(script.keys))
	}
}
