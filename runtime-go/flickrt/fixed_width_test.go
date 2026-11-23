package flickrt

import "testing"

func TestFixedWidthFieldUnboxers(t *testing.T) {
	if got := FieldInt8(nil); got != 0 {
		t.Fatalf("FieldInt8(nil) = %d, want 0", got)
	}
	if got := FieldInt8(int8(-128)); got != -128 {
		t.Fatalf("FieldInt8(-128) = %d, want -128", got)
	}
	if got := FieldInt16(nil); got != 0 {
		t.Fatalf("FieldInt16(nil) = %d, want 0", got)
	}
	if got := FieldInt16(int16(-32768)); got != -32768 {
		t.Fatalf("FieldInt16(-32768) = %d, want -32768", got)
	}
	if got := FieldInt64(nil); got != 0 {
		t.Fatalf("FieldInt64(nil) = %d, want 0", got)
	}
	if got := FieldInt64(int64(-9223372036854775808)); got != -9223372036854775808 {
		t.Fatalf("FieldInt64(min) = %d, want min", got)
	}
	if got := FieldUInt8(nil); got != 0 {
		t.Fatalf("FieldUInt8(nil) = %d, want 0", got)
	}
	if got := FieldUInt8(uint8(255)); got != 255 {
		t.Fatalf("FieldUInt8(255) = %d, want 255", got)
	}
	if got := FieldUInt16(nil); got != 0 {
		t.Fatalf("FieldUInt16(nil) = %d, want 0", got)
	}
	if got := FieldUInt16(uint16(65535)); got != 65535 {
		t.Fatalf("FieldUInt16(65535) = %d, want 65535", got)
	}
	if got := FieldUInt32(nil); got != 0 {
		t.Fatalf("FieldUInt32(nil) = %d, want 0", got)
	}
	if got := FieldUInt32(uint32(4294967295)); got != 4294967295 {
		t.Fatalf("FieldUInt32(max) = %d, want max", got)
	}
	if got := FieldUInt64(nil); got != 0 {
		t.Fatalf("FieldUInt64(nil) = %d, want 0", got)
	}
	if got := FieldUInt64(uint64(18446744073709551615)); got != 18446744073709551615 {
		t.Fatalf("FieldUInt64(max) = %d, want max", got)
	}
	if got := FieldFloat32(nil); got != 0 {
		t.Fatalf("FieldFloat32(nil) = %v, want 0", got)
	}
	if got := FieldFloat32(float32(1.5)); got != 1.5 {
		t.Fatalf("FieldFloat32(1.5) = %v, want 1.5", got)
	}
}
