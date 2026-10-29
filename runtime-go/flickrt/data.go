package flickrt

import (
	"math"
	"math/big"
	"sort"
	"strconv"
	"strings"
)

// Equals is `==`: structural equality, SameValueZero on numbers.
func Equals(a, b Value) bool {
	switch x := a.(type) {
	case float64:
		y, ok := b.(float64)
		return ok && (x == y || (x != x && y != y))
	case *big.Int:
		y, ok := b.(*big.Int)
		return ok && x.Cmp(y) == 0
	case []Value:
		y, ok := b.([]Value)
		if !ok || len(x) != len(y) {
			return false
		}
		for i := range x {
			if !Equals(x[i], y[i]) {
				return false
			}
		}
		return true
	case *Data:
		y, ok := b.(*Data)
		if !ok || x.Tag != y.Tag || len(x.Fields) != len(y.Fields) {
			return false
		}
		for i := range x.Fields {
			if !Equals(x.Fields[i], y.Fields[i]) {
				return false
			}
		}
		return true
	case Record:
		y, ok := b.(Record)
		if !ok || len(x) != len(y) {
			return false
		}
		for k, v := range x {
			w, ok := y[k]
			if !ok || !Equals(v, w) {
				return false
			}
		}
		return true
	case *Decimal:
		y, ok := b.(*Decimal)
		return ok && decCompare(x, y) == 0
	case *Duration:
		y, ok := b.(*Duration)
		return ok && x.infinite == y.infinite && (x.infinite || x.nanos.Cmp(y.nanos) == 0)
	case Fn, Poly, *Dict:
		// functions compare by identity on the JS target; Go cannot compare them
		return false
	default:
		return a == b
	}
}

// Same is `===` on values whose identity is their equality.
func Same(a, b Value) bool {
	if x, ok := a.(*big.Int); ok {
		y, ok := b.(*big.Int)
		return ok && x.Cmp(y) == 0
	}
	return a == b
}

func ordering(c int) Value {
	switch {
	case c < 0:
		return LT
	case c > 0:
		return GT
	default:
		return EQ
	}
}

// Compare is the total order of `Ord` on primitives: on numbers −Infinity <
// finite < +Infinity < NaN, signed zeros and all NaNs equal; strings by
// UTF-16 code units.
func Compare(a, b Value) Value { return ordering(compare(a, b)) }

func compare(a, b Value) int {
	switch x := a.(type) {
	case int32:
		y := b.(int32)
		switch {
		case x < y:
			return -1
		case x > y:
			return 1
		}
		return 0
	case float64:
		y := b.(float64)
		xn, yn := math.IsNaN(x), math.IsNaN(y)
		switch {
		case xn || yn:
			return boolInt(xn) - boolInt(yn)
		case x < y:
			return -1
		case x > y:
			return 1
		}
		return 0
	case string:
		return CompareStrings(x, b.(string))
	case *big.Int:
		return x.Cmp(b.(*big.Int))
	case bool:
		return boolInt(x) - boolInt(b.(bool))
	}
	panic(Defect{"compare of values without an order"})
}

func boolInt(b bool) int {
	if b {
		return 1
	}
	return 0
}

// Lt, Le, Gt and Ge are the comparison operators on primitives, as JS
// compares them: any comparison with NaN is false.
func Lt(a, b Value) bool { return ordered(a, b) && compare(a, b) < 0 }
func Le(a, b Value) bool { return ordered(a, b) && compare(a, b) <= 0 }
func Gt(a, b Value) bool { return ordered(a, b) && compare(a, b) > 0 }
func Ge(a, b Value) bool { return ordered(a, b) && compare(a, b) >= 0 }

func ordered(a, b Value) bool {
	x, ok := a.(float64)
	if !ok {
		return true
	}
	return !math.IsNaN(x) && !math.IsNaN(b.(float64))
}

// combine folds a hash into another, as Flick's list and derived hashes do.
func combine(h, x int32) int32 { return h*31 + x }

// Hash is `hash` of a primitive, as Effect's `Hash.hash` computes it on the
// JS target, which hash collections order their elements by.
func Hash(a Value) Value { return hash(a) }

func optimize(n int32) int32 { return n&-0x40000001 | int32(uint32(n)>>1)&0x40000000 }

func mix(h int32) int32 {
	u := uint32(h)
	u ^= u >> 16
	u *= 0x85ebca6b
	u ^= u >> 13
	u *= 0xc2b2ae35
	return int32(u ^ u>>16)
}

func effectCombine(a, b int32) int32 {
	return mix(int32(uint32(a)*0x9e3779b1 + uint32(b)*0x85ebca6b))
}

// hashString is Effect's djb2 over the UTF-16 code units, from the last.
func hashString(s string) int32 {
	units := utf16Units(s)
	h := int32(5381)
	for i := len(units) - 1; i >= 0; i-- {
		h = h*33 ^ int32(units[i])
	}
	return optimize(h)
}

func hashNumber(x float64) int32 {
	if i := toInt32(x); float64(i) == x {
		return optimize(i)
	}
	bits := math.Float64bits(x)
	if x != x {
		// JS writes every NaN with these bits; Go's math.NaN has others
		bits = 0x7ff8000000000000
	}
	return optimize(effectCombine(int32(bits>>32), int32(bits)))
}

func hash(a Value) int32 {
	switch x := a.(type) {
	case int32:
		return optimize(x)
	case float64:
		return hashNumber(x)
	case string:
		return hashString(x)
	case *big.Int:
		return hashString(x.String())
	case bool:
		return optimize(mix(hashString(strconv.FormatBool(x))))
	case UnitType:
		return optimize(mix(hashString("undefined")))
	case []Value:
		h := int32(6151)
		for _, v := range x {
			h = effectCombine(h, hash(v))
		}
		return optimize(h)
	case *Data:
		// the object `{ _tag, _0, … }`: its keys in any order
		h := int32(12289) ^ effectCombine(hashString("_tag"), hashString(x.Tag))
		for i, v := range x.Fields {
			h ^= effectCombine(hashString(nativeField(x.Tag, i)), hash(v))
		}
		return optimize(h)
	case Record:
		h := int32(12289)
		for k, v := range x {
			h ^= effectCombine(hashString(k), hash(v))
		}
		return optimize(h)
	}
	panic(Defect{"hash of a value without a hash"})
}

// nativeField is the JS property field `i` of a variant value is in.
func nativeField(tag string, i int) string {
	switch tag {
	case "Some":
		return "value"
	case "Success":
		return "success"
	case "Failure":
		return "failure"
	}
	return "_" + strconv.Itoa(i)
}

// Show is `show` of a primitive: a string as JSON, a BigInt with `n`, a
// variant value by its constructor's name, a number as JS writes it.
func Show(a Value) Value {
	switch x := a.(type) {
	case string:
		return QuoteJSON(x)
	case *big.Int:
		return x.String() + "n"
	case *Data:
		return ConstructorName(x.Tag)
	case int32:
		return strconv.Itoa(int(x))
	case float64:
		return ShowNumber(x)
	case bool:
		if x {
			return "true"
		}
		return "false"
	case UnitType:
		return "undefined"
	}
	return "[object Object]"
}

// ListEquals, ListCompare, ListHash and ListShow are the instances of
// lists, given those of the elements.
func ListEquals(eq, a, b Value) Value {
	xs, ys := a.([]Value), b.([]Value)
	if len(xs) != len(ys) {
		return false
	}
	for i := range xs {
		if !Bool(Apply(Apply(eq, xs[i]), ys[i])) {
			return false
		}
	}
	return true
}

func ListCompare(cmp, a, b Value) Value {
	xs, ys := a.([]Value), b.([]Value)
	for i := 0; i < len(xs) && i < len(ys); i++ {
		o := Apply(Apply(cmp, xs[i]), ys[i])
		if o.(*Data).Tag != EQ.(*Data).Tag {
			return o
		}
	}
	return ordering(len(xs) - len(ys))
}

func ListHash(h, a Value) Value {
	acc := int32(7)
	for _, x := range a.([]Value) {
		acc = combine(acc, Apply(h, x).(int32))
	}
	return acc
}

func ListShow(s, a Value) Value {
	xs := a.([]Value)
	shown := make([]string, len(xs))
	for i, x := range xs {
		shown[i] = Apply(s, x).(string)
	}
	return "[" + joinStrings(shown, ", ") + "]"
}

func joinStrings(parts []string, sep string) string { return strings.Join(parts, sep) }

// sortedKeys is the labels of a record in UTF-16 order.
func sortedKeys(r Record) []string {
	keys := make([]string, 0, len(r))
	for k := range r {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool { return CompareStrings(keys[i], keys[j]) < 0 })
	return keys
}
