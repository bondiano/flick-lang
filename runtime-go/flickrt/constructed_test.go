package flickrt

import (
	"os/exec"
	"strings"
	"testing"
)

// A constructor with a Go type of its own, as generated code writes one.
type ownPair struct{ f [2]Value }

func (p *ownPair) TagName() string { return "Pair#1" }
func (p *ownPair) Fields() []Value { return p.f[:] }

// Payload-free constructors with zero-size Go types of their own.
type (
	ownLeaf  struct{}
	ownOther struct{}
)

func (ownLeaf) TagName() string  { return "Leaf#1" }
func (ownLeaf) Fields() []Value  { return nil }
func (ownOther) TagName() string { return "Other#1" }
func (ownOther) Fields() []Value { return nil }

func isOwnLeaf(v Value) bool {
	_, ok := v.(ownLeaf)
	return ok
}

func TestAValueOfItsOwnGoTypeIsReadAsAnyConstructorValue(t *testing.T) {
	own := &ownPair{[2]Value{int32(1), "x"}}
	same, other := &ownPair{[2]Value{int32(1), "x"}}, &ownPair{[2]Value{int32(2), "x"}}
	data := NewData("Pair#1", int32(1), "x")
	if got := Show(own); got != "Pair" {
		t.Errorf("show: %v", got)
	}
	if got, want := Format(own), `{"_tag":"Pair#1","_0":1,"_1":"x"}`; got != want {
		t.Errorf("format: %v, want %v", got, want)
	}
	if !Equals(own, same) || Equals(own, other) || !Equals(own, data) || !Equals(data, own) {
		t.Errorf("equality: %v %v %v %v", Equals(own, same), Equals(own, other), Equals(own, data), Equals(data, own))
	}
	if Equals(own, NewData("Pair#2", int32(1), "x")) || Equals(own, "x") {
		t.Error("equal to a value of another constructor, or to no constructor value")
	}
	// the hash of the JS object `{ _tag, _0, _1 }`, whatever the Go type
	if got, want := hash(own), hash(data); got != want || got == hash(other) {
		t.Errorf("hash: %v, of the same fields as data %v, of other fields %v", got, want, hash(other))
	}
	if IsData(own, "Pair#1") || !IsData(data, "Pair#1") || IsData(data, "Pair#2") {
		t.Error("the tag tests")
	}
	if TagOf(own) != "Pair#1" || TagOf(data) != "Pair#1" {
		t.Errorf("tag: %v %v", TagOf(own), TagOf(data))
	}
	if n := testing.AllocsPerRun(100, func() { Equals(own, same) }); n != 0 {
		t.Errorf("%v allocations to compare, want 0", n)
	}
}

// An arm of a `try`, or of a `match` on `Errors e`, tests an error of a type
// whose values are *Data among errors of every type, some of Go types of
// their own.
func TestATagTestIsFalseOfAValueOfAnotherGoType(t *testing.T) {
	own := &ownPair{[2]Value{int32(1), "x"}}
	if IsTag(own, "ServeError#2") || !IsTag(NewData("ServeError#2", "x"), "ServeError#2") {
		t.Error("the tag test of an error")
	}
}

func TestTheRuntimeBuildsAValueInTheGoTypeOfItsConstructor(t *testing.T) {
	Owned("Pair#1", func(fields []Value) Value { return &ownPair{[2]Value{fields[0], fields[1]}} })
	defer delete(builders, "Pair#1")
	built, isOwn := Construct("Pair#1", int32(1), "x").(*ownPair)
	if !isOwn || built.f != [2]Value{int32(1), "x"} {
		t.Errorf("built %#v", built)
	}
	if d, isData := Construct("Other#1", int32(1)).(*Data); !isData || d.Tag != "Other#1" || d.Fields()[0] != int32(1) {
		t.Errorf("a constructor without a Go type of its own: %#v", d)
	}
	if a, b := Construct("NullaryUnowned#1"), Construct("NullaryUnowned#1"); a != b {
		t.Errorf("unowned nullary constructor is not canonical: %#v != %#v", a, b)
	}
}

func TestAZeroSizeConstructorAllocatesNothingAndPreservesValueSemantics(t *testing.T) {
	Owned("Leaf#1", func([]Value) Value { return ownLeaf{} })
	defer delete(builders, "Leaf#1")

	if n := testing.AllocsPerRun(100, func() { variantKept = Value(ownLeaf{}) }); n != 0 {
		t.Errorf("building zero-size constructor: %v allocations, want 0", n)
	}
	if n := testing.AllocsPerRun(100, func() { variantKept = Construct("Leaf#1") }); n != 0 {
		t.Errorf("Construct zero-size constructor: %v allocations, want 0", n)
	}
	leaf := Value(ownLeaf{})
	other := Value(ownOther{})
	data := Nullary("Leaf#1")
	if !isOwnLeaf(leaf) || isOwnLeaf(other) || isOwnLeaf(data) {
		t.Errorf("type test on zero-size constructor failed")
	}
	if n := testing.AllocsPerRun(100, func() {
		if isOwnLeaf(leaf) {
			variantCount++
		}
	}); n != 0 {
		t.Errorf("type test on zero-size constructor: %v allocations, want 0", n)
	}
	// Two values of one zero-size type are equal as Go interfaces.
	if leaf != Value(ownLeaf{}) || leaf == other {
		t.Errorf("Go interface equality failed")
	}
	if !Same(leaf, Value(ownLeaf{})) || Same(leaf, other) {
		t.Errorf("Same on zero-size constructor failed")
	}
	if !Equals(leaf, Value(ownLeaf{})) || Equals(leaf, other) || !Equals(leaf, data) || !Equals(data, leaf) {
		t.Errorf("Equals on zero-size constructor failed")
	}
	if !SameValue(leaf, Value(ownLeaf{})) || SameValue(leaf, other) || !SameValue(leaf, data) {
		t.Errorf("SameValue on zero-size constructor failed")
	}
	if Equals(leaf, "Leaf") || Equals(leaf, NewData("Other#1")) {
		t.Errorf("zero-size constructor equal to unrelated value")
	}
	if n := testing.AllocsPerRun(100, func() { Equals(leaf, Value(ownLeaf{})) }); n != 0 {
		t.Errorf("Equals on zero-size constructor: %v allocations, want 0", n)
	}
	if got := Show(leaf); got != "Leaf" {
		t.Errorf("Show: %v, want Leaf", got)
	}
	if got, want := Format(leaf), `{"_tag":"Leaf#1"}`; got != want {
		t.Errorf("Format: %v, want %v", got, want)
	}
	if got, want := hash(leaf), hash(data); got != want || got == hash(other) {
		t.Errorf("hash: %v, data %v, other %v", got, want, hash(other))
	}
	if TagOf(leaf) != "Leaf#1" || TagField(leaf) != "Leaf#1" || TagField(data) != "Leaf#1" || TagField(Rec("_tag", "rec")) != "rec" {
		t.Errorf("TagOf / TagField failed")
	}

	// Schema decoding, Schema.arbitrary, and Arbitrary.from_schema build the owned zero-size type.
	schema := SchemaAdt("Tree", []Variant{{Tag: "Leaf#1", Name: "Leaf"}})
	fb := rootFiber()
	decoded, issues := sch(schema).jsonOf().decode(fb, Rec("_tag", "Leaf"))
	if issues != nil || !isOwnLeaf(decoded) {
		t.Errorf("schema decode: %#v (issues: %v)", decoded, issues)
	}
	encoded, issues := sch(schema).jsonOf().encode(fb, leaf)
	if issues != nil || recordField(encoded.(*Record), "_tag") != "Leaf" {
		t.Errorf("schema encode: %#v (issues: %v)", encoded, issues)
	}
	validated, issues := sch(schema).validate(fb, leaf)
	if issues != nil || !isOwnLeaf(validated) {
		t.Errorf("schema validate: %#v (issues: %v)", validated, issues)
	}
	drawn := SchemaArbitrary(schema).(gen)(fb, random(42))
	if !isOwnLeaf(drawn) {
		t.Errorf("SchemaArbitrary: got %T (%#v), want ownLeaf", drawn, drawn)
	}
	samples := ArbitrarySample(fb, Rec("count", Some(int32(3)), "size", None, "maxDiscards", None, "seed", Some(int32(42))), ArbitrarySchema(schema))
	for _, sample := range List(samples) {
		if !isOwnLeaf(sample) {
			t.Errorf("ArbitraryFromSchema: got %T (%#v), want ownLeaf", sample, sample)
		}
	}
}

// Compiled code reads a value through these on every field and every arm of
// a `match`: each must stay small enough for Go to put it where it is
// called. A branch too many and it is a call, which no test of what it
// gives would show.
func TestTheReadersOfAValueAreInlined(t *testing.T) {
	if testing.Short() {
		t.Skip("compiles the package")
	}
	out, err := exec.Command("go", "build", "-gcflags=-m", ".").CombinedOutput()
	if err != nil {
		t.Fatalf("%v\n%s", err, out)
	}
	for _, f := range []string{"IsTag", "IsData", "Get", "Field", "FieldAt", "At", "Len"} {
		if !strings.Contains(string(out), ": can inline "+f+"\n") {
			t.Errorf("Go does not inline %s", f)
		}
	}
}
