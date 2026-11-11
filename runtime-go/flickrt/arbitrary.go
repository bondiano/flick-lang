package flickrt

import (
	"math/rand/v2"
	"strings"
)

// `Arbitrary` for the bindings of the standard library: the engine of
// arbitrary_*.go, so a seed draws, shrinks and reports what it does on the
// JS target. An Arbitrary value is a *arbGen.

func arb(v Value) *arbGen { return v.(*arbGen) }

// ArbitrarySchema is `Arbitrary.from_schema`: values of the schema's type.
func ArbitrarySchema(schema Value) Value { return compileSchema(sch(schema)) }

// ArbitrarySchemaWith is `Arbitrary.from_schema_with`: a `shrink` of the
// options replaces the default shrinking, its candidates validated.
func ArbitrarySchemaWith(options, schema Value) Value {
	compiled := compileSchema(sch(schema))
	shrink, ok := some(record(options).Get("shrink"))
	if !ok {
		return compiled
	}
	s := sch(schema)
	return makeGen(compiled.minCost, func(st *genState) *sample {
		a := compiled.generate(st)
		if a == nil || !st.shrinks {
			return a
		}
		return validatedShrinks(st.fb, a.value, shrink, s)
	})
}

func validatedShrinks(fb *Fiber, v, shrink Value, s *Schema) *sample {
	return lazyShrinks(v, func() shrinkPull {
		return pullEach(List(Apply(fb, shrink, v)), func(c Value) *sample {
			valid, issues := s.validate(fb, c)
			if issues != nil {
				return nil
			}
			return validatedShrinks(fb, valid, shrink, s)
		})
	})
}

// ArbitraryConstant is `Arbitrary.constant`.
func ArbitraryConstant(v Value) Value { return constantGen(v) }

// ArbitraryArray is `Arbitrary.list`.
func ArbitraryArray(options, item Value) Value {
	o := record(options)
	minimum, maximum := 0.0, float64(0xffffffff)
	if n, ok := some(o.Get("minLength")); ok {
		minimum = float64(n.(int32))
	}
	if n, ok := some(o.Get("maxLength")); ok {
		maximum = float64(n.(int32))
	}
	return arrayGen(arb(item), minimum, maximum)
}

// ArbitraryAll is `Arbitrary.all` of a list, and the tuples of `zip2`…
func ArbitraryAll(members Value) Value {
	xs := List(members)
	gens := make([]*arbGen, len(xs))
	for i, x := range xs {
		gens[i] = arb(x)
	}
	return allGen(gens)
}

func ArbitraryZip2(a, b Value) Value       { return ArbitraryAll([]Value{a, b}) }
func ArbitraryZip3(a, b, c Value) Value    { return ArbitraryAll([]Value{a, b, c}) }
func ArbitraryZip4(a, b, c, d Value) Value { return ArbitraryAll([]Value{a, b, c, d}) }

// ArbitraryMap is `Arbitrary.map`.
func ArbitraryMap(f, self Value) Value {
	return mapGen(arb(self), func(st *genState, v Value) Value { return Apply(st.fb, f, v) })
}

// ArbitraryFilter is `Arbitrary.filter`: a value the predicate rejects is
// a discard.
func ArbitraryFilter(predicate, self Value) Value {
	return filterGen(arb(self), func(st *genState, v Value) bool { return Apply(st.fb, predicate, v).(bool) })
}

// ArbitraryFilterMap is `Arbitrary.filter_map`: an `Err` is a discard.
func ArbitraryFilterMap(f, self Value) Value {
	return filterMapGen(arb(self), func(st *genState, v Value) (Value, bool) {
		r := Apply(st.fb, f, v)
		if isOk(r) {
			return value(r), true
		}
		return nil, false
	})
}

// ArbitraryFlatMap is `Arbitrary.flat_map`.
func ArbitraryFlatMap(f, self Value) Value {
	return flatMapGen(arb(self), func(st *genState, v Value) *arbGen { return arb(Apply(st.fb, f, v)) })
}

// ArbitrarySample is `Arbitrary.sample`.
func ArbitrarySample(fb *Fiber, options, self Value) Value { return arbSample(fb, arb(self), options) }

// ArbitraryCheck is `Arbitrary.check`: `property` run on values of `self`.
func ArbitraryCheck(fb *Fiber, options, property, self Value) Value {
	return arbCheck(fb, arb(self), property, options)
}

func globalRandom() *xoshiro {
	return &xoshiro{rand.Uint32(), rand.Uint32(), rand.Uint32(), rand.Uint32() | 1}
}

var (
	passedTag         = Tag("Arbitrary.CheckResult.Passed")
	falsifiedTag      = Tag("Arbitrary.CheckResult.Falsified")
	exhaustedTag      = Tag("Arbitrary.CheckResult.Exhausted")
	replayMismatchTag = Tag("Arbitrary.CheckResult.ReplayMismatch")
	returnedFalseTag  = Tag("Arbitrary.PropertyFailure.ReturnedFalse")
	propertyErrorTag  = Tag("Arbitrary.PropertyFailure.PropertyError")
	sampleErrorTag    = Tag("Arbitrary.SampleError.SampleError")
)

// evaluate runs the property: nil when it passed, else its failure. A
// defect or an interruption of the property ends the check.
func evaluate(fb *Fiber, property, v Value) Value {
	out, cause := catch(func() Value { return Run(fb, Apply(fb, property, v)) })
	if cause == nil {
		if out.(bool) {
			return nil
		}
		return Nullary(returnedFalseTag)
	}
	for _, r := range cause.reasons() {
		if r.kind != causeFail {
			panic(&Failure{cause})
		}
	}
	return NewData(propertyErrorTag, cause.firstOf(causeFail).err)
}

// ArbitraryFormatCheckFailure is `Arbitrary.format_check_failure`: the
// report of a failed check, `None` when it passed, its values formatted
// with two spaces.
func ArbitraryFormatCheckFailure(result Value) Value {
	d := result.(*Data)
	switch d.Tag {
	case falsifiedTag:
		f := record(d.Fields[0])
		failure := "Failure: returned false"
		if e := f.Get("failure").(*Data); e.Tag == propertyErrorTag {
			failure = "Failure: " + formatSpaced(e.Fields[0], 2)
		}
		return Some(strings.Join([]string{
			"Property falsified after " + jsString(f.Get("runs")) + " run(s) and " + jsString(f.Get("shrinks")) + " shrink(s)",
			"Shrunk input: " + formatSpaced(f.Get("shrunkInput"), 2),
			failure,
			"Replay: " + f.Get("replay").(string),
		}, "\n"))
	case exhaustedTag:
		f := record(d.Fields[0])
		return Some("Property exhausted after " + jsString(f.Get("runs")) + " run(s) and " +
			jsString(f.Get("discards")) + " discard(s)\nSeed: " + QuoteJSON(f.Get("seed").(string)))
	case replayMismatchTag:
		return Some("Property replay failed: " + d.Fields[0].(string))
	}
	return None
}
