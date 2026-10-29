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

func hashString(s string) int32 {
	h := int32(5381)
	for _, u := range utf16Units(s) {
		h = h*33 ^ int32(u)
	}
	return h
}

func combine(h, x int32) int32 { return h*31 + x }

// Hash is a hash consistent with Equals.
func Hash(a Value) Value { return hash(a) }

func hash(a Value) int32 {
	switch x := a.(type) {
	case int32:
		return hashString(strconv.Itoa(int(x)))
	case float64:
		if x == 0 {
			return hashString("0")
		}
		return hashString(ShowNumber(x))
	case []Value:
		h := int32(7)
		for _, v := range x {
			h = combine(h, hash(v))
		}
		return h
	case *Data:
		// as JS hashes the object `{ _tag, _0, … }` (or `{ _tag, value }`) by sorted keys
		keys := make([]string, len(x.Fields))
		for i := range x.Fields {
			keys[i] = nativeField(x.Tag, i)
		}
		order := make([]int, len(keys))
		for i := range order {
			order[i] = i
		}
		sort.Slice(order, func(i, j int) bool { return keys[order[i]] < keys[order[j]] })
		h := int32(13)
		tagged := false
		for _, i := range order {
			if !tagged && keys[i] > "_tag" {
				h = combine(combine(h, hashString("_tag")), hashString("string:"+x.Tag))
				tagged = true
			}
			h = combine(combine(h, hashString(keys[i])), hash(x.Fields[i]))
		}
		if !tagged {
			h = combine(combine(h, hashString("_tag")), hashString("string:"+x.Tag))
		}
		return h
	case Record:
		h := int32(13)
		for _, k := range sortedKeys(x) {
			h = combine(combine(h, hashString(k)), hash(x[k]))
		}
		return h
	case string:
		return hashString("string:" + x)
	case bool:
		if x {
			return hashString("boolean:true")
		}
		return hashString("boolean:false")
	case *big.Int:
		return hashString("bigint:" + x.String())
	}
	return hashString("undefined:undefined")
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
