package flickrt

import (
	"fmt"
	"math/big"
	"os"
	"strings"
	"sync"
	"testing"
)

// The port draws, shrinks and replays what Effect does: testdata/
// arbitrary.txt is the output of the same cases run by Effect 4.0.5's
// Arbitrary on Node, over the schemas the JS runtime builds.

func showArb(v Value) string {
	switch x := v.(type) {
	case float64:
		if x == 0 {
			return "0" // as JS's String(-0): an Int, int32 here, has no -0
		}
		return ShowNumber(x)
	case int32:
		return fmt.Sprint(x)
	case string:
		var b strings.Builder
		b.WriteByte('"')
		for _, u := range utf16Units(x) {
			switch {
			case u == '"':
				b.WriteString(`\"`)
			case u == '\\':
				b.WriteString(`\\`)
			case u >= 0x20 && u <= 0x7e:
				b.WriteByte(byte(u))
			default:
				fmt.Fprintf(&b, `\u%04x`, u)
			}
		}
		return b.String() + `"`
	case bool:
		return fmt.Sprint(x)
	case *big.Int:
		return x.String() + "n"
	case *Decimal:
		return fmt.Sprintf("dec(%s,%d)", x.value, x.scale)
	case *Redacted:
		return "Redacted(" + showArb(x.value) + ")"
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			parts[i] = showArb(e)
		}
		return "[" + strings.Join(parts, ",") + "]"
	case *Record:
		parts := make([]string, len(x.keys))
		for i, k := range x.keys {
			parts[i] = k + ":" + showArb(x.vals[i])
		}
		return "{" + strings.Join(parts, ",") + "}"
	case *Data:
		names := map[string]string{"Success": "Ok", "Failure": "Err"}
		tag := x.Tag
		if n, ok := names[tag]; ok {
			tag = n
		}
		if len(x.Fields) == 0 {
			return tag
		}
		parts := make([]string, len(x.Fields))
		for i, e := range x.Fields {
			parts[i] = showArb(e)
		}
		return tag + "(" + strings.Join(parts, ",") + ")"
	}
	if v == Unit {
		return "()"
	}
	return fmt.Sprintf("?%v", v)
}

func len16(v Value) int { return len(utf16Units(v.(string))) }

func withCheck(base *arbAST, c *check) *arbAST {
	return base.withChecks(arbFilter{c.arb, c.test})
}

func treeSchema(field Value) Value {
	var once sync.Once
	var adt Value
	get := func() Value {
		once.Do(func() {
			adt = SchemaAdt("Tree", []Variant{
				{Tag: "Leaf", Name: "Leaf"},
				{Tag: "Node", Name: "Node", Fields: []Value{field, SchemaList(SchemaLazy("Tree", func() Value { return adt }))}},
			})
		})
		return adt
	}
	return SchemaLazy("Tree", get)
}

type arbCase struct {
	name     string
	arb      Value
	property func(Value) bool
}

func arbCases() []arbCase {
	number := func(v Value) float64 { return v.(float64) }
	isTag := func(v Value, tag string) bool { return v.(*Data).Tag == tag }
	digit := CodecToSchema(CheckApply(CheckBetween(Rec("minimum", 0.0, "maximum", 9.0, "exclusiveMinimum", false, "exclusiveMaximum", false)),
		CheckApply(CheckInt, CodecNumberFinite)))
	between := CheckBetween(Rec("minimum", -1.5, "maximum", 2.5, "exclusiveMinimum", false, "exclusiveMaximum", false)).(*check)
	exclusive := CheckBetween(Rec("minimum", 0.0, "maximum", 1.0, "exclusiveMinimum", true, "exclusiveMaximum", true)).(*check)
	nonnegative := isGreaterThanOrEqualTo(0)
	nonnegative.arb = &arbConstraint{order: "number", minimum: 0.0}
	stringJSON := sch(SchemaString).jsonOf()
	length := CodecToSchema(CheckApply(CheckMaxLength(int32(5)), CheckApply(CheckMinLength(int32(2)), stringJSON)))
	pattern := CodecToSchema(CheckApply(CheckPattern(`^[a-c]+\d{2}$`), stringJSON))
	unanchored := CodecToSchema(CheckApply(CheckPattern(`ab+c`), stringJSON))
	nonEmpty := SchemaRefined("NonEmpty", SchemaString, Fn(func(_ *Fiber, s Value) Value { return len16(s) > 0 }), "")
	bigBetween := astBigIntNode.withChecks(arbFilter{&arbConstraint{order: "bigint", minimum: big.NewInt(-5), maximum: big.NewInt(1000)},
		func(_ *Fiber, v Value) bool {
			return v.(*big.Int).Cmp(big.NewInt(-5)) >= 0 && v.(*big.Int).Cmp(big.NewInt(1000)) <= 0
		}})
	adt := SchemaAdt("T", []Variant{{Tag: "A", Name: "A"}, {Tag: "B", Name: "B", Fields: []Value{SchemaNumber}},
		{Tag: "C", Name: "C", Fields: []Value{SchemaString, SchemaBool}}})
	s := ArbitrarySchema
	fn := func(f func(Value) Value) Value { return Fn(func(_ *Fiber, v Value) Value { return f(v) }) }
	return []arbCase{
		{"number", s(SchemaNumber), func(v Value) bool { return number(v) < 10 }},
		{"digit", s(digit), func(v Value) bool { return number(v) < 5 }},
		{"int", s(SchemaInt), func(v Value) bool { return v.(int32) < 1000 }},
		{"between", compileAST(withCheck(astNumberNode, between)), func(v Value) bool { return number(v) < 1 }},
		{"exclusive", compileAST(withCheck(astNumberNode, exclusive)), func(v Value) bool { return number(v) < 0.5 }},
		{"nonnegative", compileAST(withCheck(astNumberNode, nonnegative)), func(v Value) bool { return number(v) < 1e6 }},
		{"finite", s(CodecToSchema(CodecNumberFinite)), func(v Value) bool { return number(v) > -100 }},
		{"string", s(SchemaString), func(v Value) bool { return len16(v) < 4 }},
		{"length", s(length), func(v Value) bool { return !strings.Contains(v.(string), "a") }},
		{"pattern", s(pattern), func(v Value) bool { return len16(v) < 5 }},
		{"unanchored", s(unanchored), func(v Value) bool { return len16(v) < 8 }},
		{"nonempty", s(nonEmpty), func(v Value) bool { return len16(v) < 3 }},
		{"boolean", s(SchemaBool), func(v Value) bool { return v == false }},
		{"unit", s(SchemaUnit), func(Value) bool { return true }},
		{"bigint", s(SchemaBigint), func(v Value) bool { return v.(*big.Int).Cmp(big.NewInt(1000)) < 0 }},
		{"bigint-between", compileAST(bigBetween), func(v Value) bool { return v.(*big.Int).Cmp(big.NewInt(100)) < 0 }},
		{"decimal", s(SchemaDecimal), func(v Value) bool { return dec(v).value.Cmp(big.NewInt(1000)) < 0 }},
		{"adt", s(adt), func(v Value) bool { return !isTag(v, "C") }},
		{"record", s(SchemaRecord("a", SchemaInt, "b", SchemaString)), func(v Value) bool { return len16(record(v).Get("b")) < 2 }},
		{"list", s(SchemaList(SchemaBool)), func(v Value) bool { return len(List(v)) < 3 }},
		{"tuple", s(SchemaTuple(SchemaBool, SchemaInt)), func(v Value) bool { return !List(v)[0].(bool) }},
		{"option", s(SchemaOption(SchemaString)), func(v Value) bool { return v == None || len16(v.(*Data).Fields[0]) < 2 }},
		{"result", s(SchemaResult(SchemaString, SchemaBool)), func(v Value) bool { return isOk(v) }},
		{"redacted", s(SchemaRedacted(SchemaBool)), func(v Value) bool { return v.(*Redacted).value == false }},
		{"tree", s(treeSchema(SchemaInt)), func(v Value) bool { return isTag(v, "Leaf") || len(List(v.(*Data).Fields[1])) < 2 }},
		{"tree2", s(treeSchema(SchemaBool)), func(v Value) bool { return isTag(v, "Leaf") || len(List(v.(*Data).Fields[1])) < 2 }},
		{"record2", s(SchemaRecord("a", SchemaBool, "b", SchemaString)), func(v Value) bool { return len16(record(v).Get("b")) < 2 }},
		{"tuple2", s(SchemaTuple(SchemaBool, SchemaString)), func(v Value) bool { return !List(v)[0].(bool) || len16(List(v)[1]) < 1 }},
		{"constant", ArbitraryConstant("x"), func(Value) bool { return true }},
		{"array", ArbitraryArray(Rec("minLength", Some(int32(1)), "maxLength", Some(int32(3))), s(SchemaBool)), func(v Value) bool { return len(List(v)) < 3 }},
		{"map", ArbitraryMap(fn(func(b Value) Value {
			if b.(bool) {
				return 1.0
			}
			return 0.0
		}), s(SchemaBool)), func(v Value) bool { return number(v) == 0 }},
		{"filter", ArbitraryFilter(fn(func(n Value) Value { return n.(int32)%2 == 0 }), s(SchemaInt)), func(v Value) bool { return v.(int32) < 10 }},
		{"filterMap", ArbitraryFilterMap(fn(func(v Value) Value {
			if len16(v) > 1 {
				return Ok(float64(len16(v)))
			}
			return Err(v)
		}), s(SchemaString)), func(v Value) bool { return number(v) < 4 }},
		{"flatMap", ArbitraryFlatMap(fn(func(b Value) Value {
			if b.(bool) {
				return s(SchemaString)
			}
			return ArbitraryConstant("")
		}), s(SchemaBool)), func(v Value) bool { return len16(v) < 2 }},
		{"all", ArbitraryAll([]Value{s(SchemaBool), s(SchemaString)}), func(v Value) bool {
			return !List(v)[0].(bool) || len16(List(v)[1]) < 2
		}},
	}
}

func arbOracleLines(fb *Fiber) []string {
	var out []string
	for _, c := range arbCases() {
		property := Fn(func(_ *Fiber, v Value) Value { return Succeed(c.property(v)) })
		for _, seed := range []float64{1, 42} {
			prefix := fmt.Sprintf("%s seed %v ", c.name, seed)
			sampled, cause := catch(func() Value {
				return ArbitrarySample(fb, Rec("count", Some(int32(8)), "size", None, "maxDiscards", None, "seed", Some(seed)), c.arb)
			})
			if cause != nil {
				e := record(cause.firstOf(causeFail).err.(*Data).Fields[0])
				out = append(out, prefix+fmt.Sprintf("sample error %v %v", e.Get("generated"), e.Get("discards")))
			} else {
				out = append(out, prefix+"sample "+showArb(sampled))
			}
			options := func(seed, replay Value) Value {
				return Rec("runs", Some(int32(30)), "size", None, "maxDiscards", None, "maxShrinks", None, "seed", seed, "replay", replay)
			}
			r := ArbitraryCheck(fb, options(Some(seed), None), property, c.arb).(*Data)
			f := record(r.Fields[0])
			switch r.Tag {
			case passedTag:
				out = append(out, prefix+fmt.Sprintf("check passed %v %v", f.Get("runs"), f.Get("discards")))
			case falsifiedTag:
				out = append(out, prefix+fmt.Sprintf("check falsified %v %v %v %s %s %s", f.Get("runs"), f.Get("discards"),
					f.Get("shrinks"), showArb(f.Get("initialInput")), showArb(f.Get("shrunkInput")), f.Get("replay")))
				again := ArbitraryCheck(fb, options(None, Some(f.Get("replay"))), property, c.arb).(*Data)
				if again.Tag == falsifiedTag {
					out = append(out, prefix+"replay Falsified "+showArb(record(again.Fields[0]).Get("shrunkInput")))
				} else {
					out = append(out, prefix+"replay ReplayMismatch "+again.Fields[0].(string))
				}
			case exhaustedTag:
				out = append(out, prefix+fmt.Sprintf("check exhausted %v %v %v", f.Get("runs"), f.Get("discards"), f.Get("seed")))
			}
		}
	}
	return out
}

func TestArbitraryAsEffect(t *testing.T) {
	want, err := os.ReadFile("testdata/arbitrary.txt")
	if err != nil {
		t.Fatal(err)
	}
	wanted := strings.Split(strings.TrimSuffix(string(want), "\n"), "\n")
	got := arbOracleLines(rootFiber())
	for i := range max(len(got), len(wanted)) {
		g, w := "", ""
		if i < len(got) {
			g = got[i]
		}
		if i < len(wanted) {
			w = wanted[i]
		}
		if g != w {
			t.Errorf("line %d:\n got %s\nwant %s", i+1, g, w)
		}
	}
}
