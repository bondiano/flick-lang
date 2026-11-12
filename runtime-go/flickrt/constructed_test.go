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
