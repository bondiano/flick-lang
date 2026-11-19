package flickrt

import (
	"reflect"
	"testing"
)

func TestPackedBytesConversionsDoNotAliasLists(t *testing.T) {
	input := []Value{int32(0), int32(128), int32(255)}
	result := BytesFromList(input).(*Data)
	if result.Tag != "Success" {
		t.Fatalf("from list returned %v", result)
	}
	bytes := value(result)
	input[0] = int32(1)
	first := BytesToList(bytes).([]Value)
	first[1] = int32(2)
	want := []Value{int32(0), int32(128), int32(255)}
	if got := BytesToList(bytes); !reflect.DeepEqual(got, want) {
		t.Fatalf("list mutation changed packed bytes to %v, want %v", got, want)
	}
	for _, tc := range []struct {
		input []Value
		want  []Value
	}{
		{[]Value{int32(255), int32(-1), int32(256)}, []Value{int32(1), int32(-1)}},
		{[]Value{int32(256)}, []Value{int32(0), int32(256)}},
	} {
		if got := BytesFromList(tc.input); !reflect.DeepEqual(got, Err(tc.want)) {
			t.Errorf("from list %v returned %v, want failure %v", tc.input, got, tc.want)
		}
	}
	if got := BytesFromList([]Value{}); !reflect.DeepEqual(got, Ok(BytesEmpty)) {
		t.Errorf("empty input returned %v", got)
	}
}

func TestPackedBytesBounds(t *testing.T) {
	bytes := BytesEncodeUtf8("abc")
	if got := BytesLength(bytes); got != int32(3) {
		t.Errorf("length is %v, want 3", got)
	}
	for _, tc := range []struct {
		index int32
		want  Value
	}{
		{-1, None}, {0, Some(int32('a'))}, {2, Some(int32('c'))}, {3, None},
	} {
		if got := BytesGet(tc.index, bytes); !reflect.DeepEqual(got, tc.want) {
			t.Errorf("get %d returned %v, want %v", tc.index, got, tc.want)
		}
	}
	for _, tc := range []struct {
		start, end int32
		want       Value
	}{
		{-1, 1, None}, {0, 4, None}, {2, 1, None}, {4, 4, None},
		{0, 0, Some("")}, {3, 3, Some("")}, {1, 3, Some("bc")},
	} {
		got := None
		if slice, ok := some(BytesSlice(tc.start, tc.end, bytes)); ok {
			got = BytesDecodeUtf8(slice)
			if out := BytesToList(slice).([]Value); len(out) > 0 {
				out[0] = int32('x')
			}
		}
		if !reflect.DeepEqual(got, tc.want) {
			t.Errorf("slice [%d,%d) returned %v, want %v", tc.start, tc.end, got, tc.want)
		}
	}
	if got := BytesDecodeUtf8(bytes); !reflect.DeepEqual(got, Some("abc")) {
		t.Errorf("slice conversion changed the original to %v", got)
	}
}

func TestPackedBytesEncodingsAndEquality(t *testing.T) {
	text := BytesEncodeUtf8("\ufeffé\xed\xa0\x80")
	if got := BytesDecodeUtf8(text); !reflect.DeepEqual(got, Some("\ufeffé\ufffd")) {
		t.Errorf("UTF-8 did not preserve BOM and replace lone surrogate: %v", got)
	}
	binary := value(BytesFromList([]Value{int32(251), int32(255)}))
	if got := BytesDecodeUtf8(binary); got != None {
		t.Errorf("invalid UTF-8 decoded to %v", got)
	}
	if got := BytesEncodeBase64URL(binary); got != "-_8" {
		t.Errorf("base64url encoding is %v, want -_8", got)
	}
	for _, text := range []string{"-_8", "-_8=", "-_9="} {
		got, ok := some(BytesDecodeBase64URL(text))
		if !ok || BytesEqualConstantTime(got, binary) != true {
			t.Errorf("base64url input %q did not decode to the expected bytes", text)
		}
	}
	for _, text := range []string{"=", "a", "-_8==", "-_8\n", "+/8=", "- =_"} {
		if got := BytesDecodeBase64URL(text); got != None {
			t.Errorf("invalid base64url input %q decoded to %v", text, got)
		}
	}
	if got := BytesDecodeBase64URL(""); !reflect.DeepEqual(got, Some(BytesEmpty)) {
		t.Errorf("empty base64url input returned %v", got)
	}
	for _, tc := range []struct {
		left, right string
		want        bool
	}{
		{"", "", true}, {"ab", "ab", true}, {"ab", "xb", false}, {"ab", "ax", false}, {"ab", "abc", false},
	} {
		if got := BytesEqualConstantTime(BytesEncodeUtf8(tc.left), BytesEncodeUtf8(tc.right)); got != tc.want {
			t.Errorf("compare %q and %q returned %v, want %v", tc.left, tc.right, got, tc.want)
		}
	}
}
