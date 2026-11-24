package flickrt

import (
	"math"
	"testing"
)

// Generated scalar-field constructors matching what own.rs emits.
type ownCircle struct {
	f0 float64
}

func (p *ownCircle) TagName() string { return "Circle#1" }
func (p *ownCircle) Fields() []Value { return []Value{p.f0} }
func (p *ownCircle) EqualConstructed(other Value, strictZero bool) (bool, bool) {
	q, ok := other.(*ownCircle)
	if !ok {
		return false, false
	}
	return EqualFloat(p.f0, q.f0, strictZero), true
}

func newOwnCircle(f0 float64) Value { return &ownCircle{f0: f0} }

type ownRect struct {
	f0 float64
	f1 float64
}

func (p *ownRect) TagName() string { return "Rect#1" }
func (p *ownRect) Fields() []Value { return []Value{p.f0, p.f1} }
func (p *ownRect) EqualConstructed(other Value, strictZero bool) (bool, bool) {
	q, ok := other.(*ownRect)
	if !ok {
		return false, false
	}
	return EqualFloat(p.f0, q.f0, strictZero) && EqualFloat(p.f1, q.f1, strictZero), true
}

func newOwnRect(f0, f1 float64) Value { return &ownRect{f0: f0, f1: f1} }

type ownEntry struct {
	f3 string
	f1 float64
	f0 int32
	f2 bool
}

func (p *ownEntry) TagName() string { return "Entry#1" }
func (p *ownEntry) Fields() []Value { return []Value{p.f0, p.f1, p.f2, p.f3} }
func (p *ownEntry) EqualConstructed(other Value, strictZero bool) (bool, bool) {
	q, ok := other.(*ownEntry)
	if !ok {
		return false, false
	}
	return p.f0 == q.f0 && EqualFloat(p.f1, q.f1, strictZero) && p.f2 == q.f2 && p.f3 == q.f3, true
}

var scalarSink [2]Value

func TestUnboxedScalarConstructorsReduceEscapingAllocsAndCompareWithoutReboxing(t *testing.T) {
	Owned("Circle#1", func(f []Value) Value { return &ownCircle{f0: AsFloat(f[0])} })
	Owned("Rect#1", func(f []Value) Value { return &ownRect{f0: AsFloat(f[0]), f1: AsFloat(f[1])} })
	Owned("Entry#1", func(f []Value) Value {
		return &ownEntry{f0: AsInt(f[0]), f1: AsFloat(f[1]), f2: AsBool(f[2]), f3: AsString(f[3])}
	})
	defer delete(builders, "Circle#1")
	defer delete(builders, "Rect#1")
	defer delete(builders, "Entry#1")

	r, w, h := 1.5, 2.5, 3.5
	// Escaping Circle Float + Rect Float Float allocates 2 objects total, not 5.
	if n := testing.AllocsPerRun(100, func() {
		scalarSink[0] = newOwnCircle(r)
		scalarSink[1] = newOwnRect(w, h)
	}); n != 2 {
		t.Errorf("escaping Circle+Rect: %v allocations, want 2", n)
	}

	c1, c2 := newOwnCircle(1.5), newOwnCircle(1.5)
	rect1, rect2 := newOwnRect(2.5, 3.5), newOwnRect(2.5, 3.5)
	rectNegZero := newOwnRect(math.Copysign(0, -1), 0.0)
	rectPosZero := newOwnRect(0.0, 0.0)
	rectNaN1, rectNaN2 := newOwnRect(math.NaN(), 1.0), newOwnRect(math.NaN(), 1.0)
	rectData := NewData("Rect#1", 2.5, 3.5)

	if !Equals(c1, c2) || Equals(c1, rect1) || !Equals(rect1, rect2) {
		t.Errorf("basic Equals failed")
	}
	if !Equals(rectNegZero, rectPosZero) {
		t.Errorf("Equals must treat -0.0 and +0.0 as equal (SameValueZero)")
	}
	if SameValue(rectNegZero, rectPosZero) {
		t.Errorf("SameValue must distinguish -0.0 and +0.0")
	}
	if !Equals(rectNaN1, rectNaN2) || !SameValue(rectNaN1, rectNaN2) {
		t.Errorf("Equals/SameValue must treat NaN == NaN")
	}
	if !Equals(rect1, rectData) || !Equals(rectData, rect1) {
		t.Errorf("cross-representation equality with *Data failed")
	}
	if n := testing.AllocsPerRun(100, func() {
		if !Equals(rect1, rect2) || !SameValue(rect1, rect2) {
			t.Fatal("unexpected inequality")
		}
	}); n != 0 {
		t.Errorf("Equals/SameValue on scalar constructor: %v allocations, want 0", n)
	}

	e1 := &ownEntry{f0: 42, f1: -0.5, f2: true, f3: "hello"}
	e2 := &ownEntry{f0: 42, f1: -0.5, f2: true, f3: "hello"}
	eData := NewData("Entry#1", int32(42), -0.5, true, "hello")
	if !Equals(e1, e2) || !Equals(e1, eData) || hash(e1) != hash(eData) {
		t.Errorf("Entry equality/hash mismatch")
	}
	if got, want := Format(e1), `{"_tag":"Entry#1","_0":42,"_1":-0.5,"_2":true,"_3":"hello"}`; got != want {
		t.Errorf("Format(e1) = %s, want %s", got, want)
	}
	if got := Show(e1); got != "Entry" {
		t.Errorf("Show(e1) = %v, want Entry", got)
	}
	if n := testing.AllocsPerRun(100, func() {
		if !Equals(e1, e2) {
			t.Fatal("unexpected inequality")
		}
	}); n != 0 {
		t.Errorf("Equals on Entry: %v allocations, want 0", n)
	}

	// Curried constructor boundary handles nil arguments from tagOf without panicking.
	curriedRect := Fn2(func(_ *Fiber, a, b Value) Value {
		return newOwnRect(FieldFloat(a), FieldFloat(b))
	})
	if got := tagOf(curriedRect); got != "Rect#1" {
		t.Errorf("tagOf(curriedRect) = %q, want Rect#1", got)
	}
	if FieldInt(nil) != 0 || FieldFloat(nil) != 0 || FieldBool(nil) != false || FieldString(nil) != "" {
		t.Errorf("Field*(nil) did not return zero values")
	}

	// Schema decode, encode, validate, and arbitrary round-trip into the owned scalar struct.
	schema := SchemaAdt("Shape", []Variant{
		{Tag: "Circle#1", Name: "Circle", Fields: []Value{SchemaNumber}},
		{Tag: "Rect#1", Name: "Rect", Fields: []Value{SchemaNumber, SchemaNumber}},
	})
	fb := rootFiber()
	decoded, issues := sch(schema).jsonOf().decode(fb, Rec("_tag", "Rect", "_0", 2.5, "_1", 3.5))
	if issues != nil {
		t.Fatalf("schema decode issues: %v", issues)
	}
	if got, ok := decoded.(*ownRect); !ok || got.f0 != 2.5 || got.f1 != 3.5 {
		t.Errorf("schema decode got %T (%#v), want *ownRect{2.5, 3.5}", decoded, decoded)
	}
	encoded, issues := sch(schema).jsonOf().encode(fb, decoded)
	if issues != nil || recordField(encoded.(*Record), "_0") != 2.5 || recordField(encoded.(*Record), "_1") != 3.5 {
		t.Errorf("schema encode got %#v (issues: %v)", encoded, issues)
	}
	validated, issues := sch(schema).validate(fb, decoded)
	if issues != nil {
		t.Fatalf("schema validate issues: %v", issues)
	}
	if _, ok := validated.(*ownRect); !ok {
		t.Errorf("schema validate got %T, want *ownRect", validated)
	}
}

func TestTypedScalarDerivedHelpersMatchBoxedSemanticsWithoutBoxing(t *testing.T) {
	floats := []float64{math.Inf(-1), -1.5, math.Copysign(0, -1), 0.0, 1.5, math.Inf(1), math.NaN()}
	for _, a := range floats {
		if got, want := HashFloat(a), Hash(a).(int32); got != want {
			t.Errorf("HashFloat(%v) = %d, want %d", a, got, want)
		}
		if got, want := ShowFloat(a), Show(a).(string); got != want {
			t.Errorf("ShowFloat(%v) = %q, want %q", a, got, want)
		}
		for _, b := range floats {
			if got, want := CompareFloat(a, b), Compare(a, b); got != want {
				t.Errorf("CompareFloat(%v, %v) = %v, want %v", a, b, got, want)
			}
			if got, want := EqFloat(a, b), Equals(a, b); got != want {
				t.Errorf("EqFloat(%v, %v) = %v, want %v", a, b, got, want)
			}
		}
	}
	ints := []int32{-2147483648, -1, 0, 1, 42, 2147483647}
	for _, a := range ints {
		if got, want := HashInt(a), Hash(a).(int32); got != want {
			t.Errorf("HashInt(%d) = %d, want %d", a, got, want)
		}
		if got, want := ShowInt(a), Show(a).(string); got != want {
			t.Errorf("ShowInt(%d) = %q, want %q", a, got, want)
		}
		for _, b := range ints {
			if got, want := CompareInt(a, b), Compare(a, b); got != want {
				t.Errorf("CompareInt(%d, %d) = %v, want %v", a, b, got, want)
			}
		}
	}
	for _, a := range []bool{false, true} {
		if got, want := HashBool(a), Hash(a).(int32); got != want {
			t.Errorf("HashBool(%v) = %d, want %d", a, got, want)
		}
		if got, want := ShowBool(a), Show(a).(string); got != want {
			t.Errorf("ShowBool(%v) = %q, want %q", a, got, want)
		}
		for _, b := range []bool{false, true} {
			if got, want := CompareBool(a, b), Compare(a, b); got != want {
				t.Errorf("CompareBool(%v, %v) = %v, want %v", a, b, got, want)
			}
		}
	}
	strs := []string{"", "a", "b", "hello \"world\"\n", "\uFF00", "\U0001F600"}
	for _, a := range strs {
		if got, want := HashString(a), Hash(a).(int32); got != want {
			t.Errorf("HashString(%q) = %d, want %d", a, got, want)
		}
		if got, want := ShowString(a), Show(a).(string); got != want {
			t.Errorf("ShowString(%q) = %q, want %q", a, got, want)
		}
		for _, b := range strs {
			if got, want := CompareString(a, b), Compare(a, b); got != want {
				t.Errorf("CompareString(%q, %q) = %v, want %v", a, b, got, want)
			}
		}
	}
	// Lone surrogate pair joining matches Concat.
	hi := fromUnits([]uint16{0xD83D})
	lo := fromUnits([]uint16{0xDE00})
	if got, want := ConcatString(hi, lo), Concat(hi, lo).(string); got != want || got != "\U0001F600" {
		t.Errorf("ConcatString surrogate pair = %q, want %q", got, want)
	}

	var hashSink int32
	if n := testing.AllocsPerRun(100, func() {
		if CompareFloat(2.5, 3.5) != LT || CompareInt(10, 10) != EQ || CompareBool(false, true) != LT || CompareString("abc", "abd") != LT {
			t.Fatal("unexpected ordering")
		}
		hashSink += HashFloat(2.5) + HashInt(42) + HashBool(true) + HashString("hello")
	}); n != 0 {
		t.Errorf("typed compare/hash helpers: %v allocations, want 0", n)
	}
	_ = hashSink
}
