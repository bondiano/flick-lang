package flickrt

import "testing"

// A field's missing-key policy: nullish and on_missing fill it in, null
// alone does not.
func TestFieldMissingPolicies(t *testing.T) {
	fb := rootFiber()
	calls := 0
	def := Fn(func(_ *Fiber, _ Value) Value { calls++; return 7.0 })
	b := CodecBlock("Row", []Value{
		"a", DslField("a", CodecOptionNullish(CodecString)),
		"b", DslOnMissing(def, DslField("b", CodecNumberLossless)),
		"c", DslField("c", CodecOptionNull(CodecString)),
	}).(*block)
	if _, issues := b.decode(fb, Rec()); len(issues) != 1 || issues[0] != (issue{"c", "a missing key"}) {
		t.Fatalf("a missing null-option key gave %v", issues)
	}
	v, issues := b.decode(fb, Rec("c", JsonNull))
	if issues != nil || isSome(record(v).Get("a")) || record(v).Get("b") != 7.0 || calls != 2 {
		t.Fatalf("decode gave %v %v after %d calls", v, issues, calls)
	}
	if _, issues := b.decode(fb, Rec("c", 3.0)); len(issues) != 1 || issues[0].message != "expected Union, got number" {
		t.Fatalf("a wrong value gave %v", issues)
	}
}

// Decoding refuses a value met again on its own path, not one shared by two.
func TestInertInput(t *testing.T) {
	items := make([]Value, 1)
	node := Rec("next", items)
	items[0] = node
	if p := inertProblem(node); len(p) != 1 || p[0] != (issue{"next[0]", "a cycle in the input"}) {
		t.Fatalf("a cycle gave %v", p)
	}
	leaf := Rec("x", 1.0)
	if p := inertProblem([]Value{leaf, leaf}); p != nil {
		t.Fatalf("a shared value gave %v", p)
	}
}

// An async validator stops once the owner it was made under has closed.
func TestStandardSchemaScope(t *testing.T) {
	owner := newOwner()
	s := CodecStandardAsync(&Fiber{state: newState(), owner: owner}, CodecString).(*StandardSchema)
	if got := Format(s.Validate("x")); got != `{"value":"x"}` {
		t.Fatalf("open gave %s", got)
	}
	owner.open = false
	if got := Format(s.Validate("x")); got != `{"issues":[{"message":"the scope of this validator has closed"}]}` {
		t.Fatalf("closed gave %s", got)
	}
}
