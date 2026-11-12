package flickrt

import (
	"reflect"
	"testing"
)

func TestAVariantHasTheFieldsItWasMadeWith(t *testing.T) {
	for n := 0; n <= 5; n++ {
		fields := make([]Value, n)
		for i := range fields {
			fields[i] = int32(i * 10)
		}
		d := NewData("A#0", fields...).(*Data)
		if d.Tag != "A#0" || len(d.Fields()) != n {
			t.Fatalf("%d fields: tag %q and %d fields", n, d.Tag, len(d.Fields()))
		}
		// the list the caller gave is the caller's: changing it changes no value
		for i := range fields {
			fields[i] = "changed"
		}
		for i := 0; i < n; i++ {
			if want := int32(i * 10); d.Fields()[i] != want || Get(d, i) != want {
				t.Errorf("%d fields: field %d is %v and %v, want %v", n, i, d.Fields()[i], Get(d, i), want)
			}
		}
	}
	for name, made := range map[string]Value{"Some": Some("v"), "Success": Ok("v"), "Failure": Err("v")} {
		if d := made.(*Data); d.Tag != name || !reflect.DeepEqual(d.Fields(), []Value{"v"}) {
			t.Errorf("%s: %q %v", name, d.Tag, d.Fields())
		}
	}
	// what is appended to the fields of a value is no field of it
	one := Some("v").(*Data)
	grown := append(one.Fields(), "more")
	if len(one.Fields()) != 1 || Get(one, 0) != "v" || len(grown) != 2 || one.in[1] != nil {
		t.Errorf("appending to the fields changed the value: %v, %v", one.Fields(), one.in)
	}
	// a field a value does not have is no field, however many it has room for
	for n := 0; n <= 3; n++ {
		d := NewData("A#0", make([]Value, n)...)
		func() {
			defer func() {
				if recover() == nil {
					t.Errorf("field %d of a value of %d fields was read", n, n)
				}
			}()
			Get(d, n)
		}()
	}
	if none := None.(*Data); len(none.Fields()) != 0 || len(Nullary("B#1").(*Data).Fields()) != 0 {
		t.Errorf("a constructor without fields has fields")
	}
}

var (
	variantKept  Value
	variantCount int
)

// A value of one or two fields is one allocation where it is kept and none
// where it is taken apart at once, as `match List.get i xs with` does.
func TestAVariantOfFewFieldsIsOneAllocationKeptAndNoneUsedAtOnce(t *testing.T) {
	x, y, z := Value("x"), Value("y"), Value("z")
	kept := map[string]func(){
		"Some":      func() { variantKept = Some(x) },
		"Ok":        func() { variantKept = Ok(x) },
		"Err":       func() { variantKept = Err(x) },
		"NewData/1": func() { variantKept = NewData("A#0", x) },
		"NewData/2": func() { variantKept = NewData("A#0", x, y) },
	}
	for name, make := range kept {
		if n := testing.AllocsPerRun(100, make); n != 1 {
			t.Errorf("%s kept: %v allocations, want 1", name, n)
		}
	}
	if n := testing.AllocsPerRun(100, func() { variantKept = NewData("A#0", x, y, z) }); n != 2 {
		t.Errorf("NewData/3 kept: %v allocations, want 2", n)
	}
	used := map[string]func(){
		"Some":      func() { variantCount += len(Some(x).(*Data).Fields()) },
		"NewData/2": func() { variantCount += len(NewData("A#0", x, y).(*Data).Fields()) },
		"Get":       func() { variantCount += len(Get(NewData("A#0", x, y), 1).(string)) },
		"IsTag": func() {
			if IsTag(Some(x), "Some") {
				variantCount++
			}
		},
	}
	for name, use := range used {
		if n := testing.AllocsPerRun(100, use); n != 0 {
			t.Errorf("%s used at once: %v allocations, want 0", name, n)
		}
	}
}
