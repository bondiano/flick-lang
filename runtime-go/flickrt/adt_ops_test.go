package flickrt

import (
	"math"
	"testing"
)

func TestAdtScalarOpsMatchGenericRuntime(t *testing.T) {
	// Float total order and equality including NaN, -0.0, +0.0, +/-Inf.
	negZero := math.Copysign(0, -1)
	floats := []float64{math.Inf(-1), -1.5, negZero, 0.0, 1.5, math.Inf(1), math.NaN()}
	for _, a := range floats {
		for _, b := range floats {
			if got, want := Ordering(CmpFloat(a, b)), Compare(a, b); got != want {
				t.Fatalf("CmpFloat(%v, %v) = %v, want %v", a, b, got, want)
			}
			a32, b32 := float32(a), float32(b)
			if got, want := Ordering(CmpFloat32(a32, b32)), Compare(a32, b32); got != want {
				t.Fatalf("CmpFloat32(%v, %v) = %v, want %v", a32, b32, got, want)
			}
			if got, want := EqualFloat32(a32, b32, true), SameValue(a32, b32); got != want {
				t.Fatalf("EqualFloat32(%v, %v, true) = %v, want %v", a32, b32, got, want)
			}
			if got, want := EqualFloat32(a32, b32, false), Equals(a32, b32); got != want {
				t.Fatalf("EqualFloat32(%v, %v, false) = %v, want %v", a32, b32, got, want)
			}
		}
		f32 := float32(a)
		if got, want := HashFloat32(f32), Hash(f32).(int32); got != want {
			t.Fatalf("HashFloat32(%v) = %v, want %v", f32, got, want)
		}
		if got, want := ShowFloat32(f32), Show(f32).(string); got != want {
			t.Fatalf("ShowFloat32(%v) = %q, want %q", f32, got, want)
		}
	}

	// Fixed-width integer ops match generic Compare/Hash/Show.
	if HashInt8(-128) != Hash(int8(-128)).(int32) || ShowInt8(-128) != "-128" {
		t.Fatal("int8 mismatch")
	}
	if HashInt16(-32000) != Hash(int16(-32000)).(int32) || ShowInt16(-32000) != "-32000" {
		t.Fatal("int16 mismatch")
	}
	if HashInt64(-9000000000000) != Hash(int64(-9000000000000)).(int32) || ShowInt64(-9000000000000) != "-9000000000000" {
		t.Fatal("int64 mismatch")
	}
	if HashUInt8(255) != Hash(uint8(255)).(int32) || ShowUInt8(255) != "255" {
		t.Fatal("uint8 mismatch")
	}
	if HashUInt16(65535) != Hash(uint16(65535)).(int32) || ShowUInt16(65535) != "65535" {
		t.Fatal("uint16 mismatch")
	}
	if HashUInt32(3000000000) != Hash(uint32(3000000000)).(int32) || ShowUInt32(3000000000) != "3000000000" {
		t.Fatal("uint32 mismatch")
	}
	if HashUInt64(18000000000000000000) != Hash(uint64(18000000000000000000)).(int32) {
		t.Fatal("uint64 hash mismatch")
	}

	// ShowArg parenthesization matches Instances.show_arg.
	if ShowArg("42") != "42" || ShowArg("Circle 2.5") != "(Circle 2.5)" || ShowArg(`"a b"`) != `"a b"` || ShowArg("{ x = 1 }") != "{ x = 1 }" {
		t.Fatal("ShowArg mismatch")
	}

	// Typed Option helpers.
	if CmpOptBool(OptBoolNone, OptBoolFalse) >= 0 || CmpOptBool(OptBoolFalse, OptBoolTrue) >= 0 {
		t.Fatal("CmpOptBool ordering mismatch")
	}
	if ShowOptBool(OptBoolNone) != "None" || ShowOptBool(OptBoolFalse) != "Some false" || ShowOptBool(OptBoolTrue) != "Some true" {
		t.Fatal("ShowOptBool mismatch")
	}
	if HashOptBool(OptBoolNone) != 0 || HashOptBool(OptBoolFalse) != HashBool(false) || HashOptBool(OptBoolTrue) != HashBool(true) {
		t.Fatal("HashOptBool mismatch")
	}
	if !EqualWithOptFloat(SomeOpt(0.0), SomeOpt(negZero), false) || EqualWithOptFloat(SomeOpt(0.0), SomeOpt(negZero), true) {
		t.Fatal("EqualWithOptFloat zero strictness mismatch")
	}
	if FieldOptInt(nil).Ok || FieldOptBool(nil) != OptBoolNone {
		t.Fatal("FieldOpt nil safety mismatch")
	}
}
