package flickrt

import (
	"math"
	"math/big"
	"testing"
)

// A property's case reads as the JS runner's JSON.stringify writes it.
func TestPreviewWritesCasesAsTheJSRunner(t *testing.T) {
	for _, c := range []struct {
		value Value
		want  string
	}{
		{[]Value{math.NaN(), math.Inf(-1), math.Copysign(0, -1), 1.5}, "[null,null,0,1.5]"},
		{[]Value{big.NewInt(-5), Unit}, `["-5n",null]`},
		{Rec("a", Unit, "b", int32(1)), `{"b":1}`},
		{[]Value{Some(int32(1)), None}, `[{"_id":"Option","_tag":"Some","value":1},{"_id":"Option","_tag":"None"}]`},
		{&Data{Tag: "Failure", Fields: []Value{"e"}}, `{"_id":"Result","_tag":"Failure","failure":"e"}`},
		{Fn(func(fb *Fiber, v Value) Value { return v }), `"<function>"`},
		{Unit, "undefined"},
	} {
		if got := preview(c.value); got != c.want {
			t.Errorf("preview = %s, want %s", got, c.want)
		}
	}
}
