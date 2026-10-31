package flickrt

import (
	"math"
	"math/big"
	"math/rand/v2"
	"strings"
)

// Effect's `Arbitrary`: generators of values, and property checks with
// Effect's schedule of runs, sizes and discards and its results and report.
//
// shortcut: values are drawn with mulberry32 by generators of this file, not by Effect's engine, so a seed draws other values than on JS, a failure is not shrunk (`shrinks` 0, the shrunk input is the initial one), and generation ignores what a schema's checks state (a refined value that fails its check is a discard); port Effect's engine (internal/arbitrary) if a program needs JS's values or shrinking

// arbitrary draws a value at a size, or false for a discarded attempt.
type arbitrary func(fb *Fiber, r rng, size int) (Value, bool)

// ArbitrarySchema is `Arbitrary.from_schema`: values of the schema's type.
func ArbitrarySchema(schema Value) Value {
	s := sch(schema)
	return arbitrary(func(fb *Fiber, r rng, size int) (Value, bool) { return s.draw(fb, r, size, 0) })
}

// ArbitraryFilter is `Arbitrary.filter`: a value the predicate rejects is
// a discard.
func ArbitraryFilter(predicate, self Value) Value {
	each := self.(arbitrary)
	return arbitrary(func(fb *Fiber, r rng, size int) (Value, bool) {
		v, ok := each(fb, r, size)
		return v, ok && Apply(fb, predicate, v).(bool)
	})
}

// below is an integer in [0, n].
func below(r rng, n int) int { return int(math.Floor(r() * float64(n+1))) }

// stringEdgeCases are Effect's (after fast-check's): a lone surrogate is
// its WTF-8 form.
var stringEdgeCases = []string{
	"", " ", "\t", "\n", "\x00", "0", "-1", "4294967295", "__proto__", "constructor",
	"prototype", "toString", "\xed\xa0\x80", "\xed\xb0\x80", "😀",
}

// numberEdgeCases are numbers a URL, a codec or arithmetic gets wrong
// first. shortcut: no NaN, which Effect draws for `Schema.Number`; add it with Effect's engine
var numberEdgeCases = []float64{
	0, math.Copysign(0, -1), 1, -1, math.Inf(1), math.Inf(-1), math.MaxFloat64, -math.MaxFloat64,
	math.SmallestNonzeroFloat64, 0.5,
}

// draw is a value of the schema, past depth 3 of a variant type's fields
// preferring a constructor without any.
func (s *Schema) draw(fb *Fiber, r rng, size, depth int) (Value, bool) {
	s = s.resolved()
	bound := size * size
	switch s.kind {
	case kindNumber:
		switch {
		case r() < 0.1:
			return numberEdgeCases[below(r, len(numberEdgeCases)-1)], true
		case r() < 0.5:
			return float64(below(r, 2*bound) - bound), true
		}
		return (r()*2 - 1) * math.Pow(10, r()*float64(2*size)-float64(size)), true
	case kindInt:
		return int32(below(r, 2*bound) - bound), true
	case kindString:
		if r() < 0.125 {
			var fits []string
			for _, e := range stringEdgeCases {
				if len(utf16Units(e)) <= size {
					fits = append(fits, e)
				}
			}
			return fits[below(r, len(fits)-1)], true
		}
		b := make([]byte, below(r, size))
		for i := range b {
			b[i] = byte(32 + below(r, 94))
		}
		return string(b), true
	case kindBool:
		return r() < 0.5, true
	case kindBigint:
		return big.NewInt(int64(below(r, 2*bound) - bound)), true
	case kindUnit:
		return Unit, true
	case kindList:
		return drawAll(fb, r, size, depth, below(r, size), func(int) *Schema { return s.item })
	case kindTuple:
		return drawAll(fb, r, size, depth, len(s.items), func(i int) *Schema { return s.items[i] })
	case kindOption:
		if r() < 0.3 {
			return None, true
		}
		v, ok := s.item.draw(fb, r, size, depth+1)
		return Some(v), ok
	case kindResult:
		if r() < 0.5 {
			v, ok := s.items[0].draw(fb, r, size, depth+1)
			return Err(v), ok
		}
		v, ok := s.items[1].draw(fb, r, size, depth+1)
		return Ok(v), ok
	case kindRedacted:
		v, ok := s.item.draw(fb, r, size, depth+1)
		return &Redacted{v}, ok
	case kindRecord:
		vs, ok := drawAll(fb, r, size, depth, len(s.items), func(i int) *Schema { return s.items[i] })
		if !ok {
			return nil, false
		}
		return shaped(s.keys, s.pos, vs.([]Value)), true
	case kindAdt:
		at := s.variants
		if depth > 3 {
			var leaves []Variant
			for _, v := range s.variants {
				if len(v.Fields) == 0 {
					leaves = append(leaves, v)
				}
			}
			if len(leaves) > 0 {
				at = leaves
			}
		}
		v := at[below(r, len(at)-1)]
		if len(v.Fields) == 0 {
			return Nullary(v.Tag), true
		}
		fields, ok := drawAll(fb, r, size, depth, len(v.Fields), func(i int) *Schema { return sch(v.Fields[i]) })
		if !ok {
			return nil, false
		}
		return NewData(v.Tag, fields.([]Value)...), true
	case kindRefined:
		v, ok := s.item.draw(fb, r, size, depth)
		return v, ok && s.refine(fb, v) == nil
	}
	panic(Defect{"Error: Arbitrary.from_schema: no generator of this schema"})
}

func drawAll(fb *Fiber, r rng, size, depth, n int, schema func(int) *Schema) (Value, bool) {
	out := make([]Value, n)
	for i := range out {
		v, ok := schema(i).draw(fb, r, size, depth+1)
		if !ok {
			return nil, false
		}
		out[i] = v
	}
	return out, true
}

// attemptRandom is the generator of an attempt of a check of a seed.
func attemptRandom(seed float64, attempt int) rng {
	h := uint32(0x811c9dc5)
	for _, c := range []byte("number:" + jsString(seed)) {
		h = (h ^ uint32(c)) * 0x01000193
	}
	return random(h + uint32(attempt)*0x9e3779b9)
}

func countOr(v Value, otherwise int) int {
	if n, ok := some(v); ok {
		return int(n.(int32))
	}
	return otherwise
}

var (
	passedTag         = Tag("Arbitrary.CheckResult.Passed")
	falsifiedTag      = Tag("Arbitrary.CheckResult.Falsified")
	exhaustedTag      = Tag("Arbitrary.CheckResult.Exhausted")
	replayMismatchTag = Tag("Arbitrary.CheckResult.ReplayMismatch")
	returnedFalseTag  = Tag("Arbitrary.PropertyFailure.ReturnedFalse")
	propertyErrorTag  = Tag("Arbitrary.PropertyFailure.PropertyError")
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

// ArbitraryCheck is `Arbitrary.check`, Effect's `checkEffect`.
//
// shortcut: invalid counts (a negative `runs`, …) are taken as they are where Effect throws a RangeError; validate them if a program relies on the error
func ArbitraryCheck(fb *Fiber, options, property, self Value) Value {
	o, each := record(options), self.(arbitrary)
	if replay, ok := some(o.Get("replay")); ok {
		return replayCheck(fb, replay.(string), property, each)
	}
	runsTarget := countOr(o.Get("runs"), 100)
	size := countOr(o.Get("size"), 10)
	maxDiscards := countOr(o.Get("maxDiscards"), max(100, runsTarget*10))
	seed := float64(rand.Int32())
	if s, ok := some(o.Get("seed")); ok {
		seed = s.(float64)
	}
	runs, discards := 0, 0
	for attempt := 0; runs < runsTarget; attempt++ {
		currentSize := size
		if runsTarget != 1 {
			currentSize = int(math.Floor(float64(runs*size)/float64(runsTarget-1) + 0.5))
		}
		v, ok := each(fb, attemptRandom(seed, attempt), currentSize)
		if !ok {
			if discards++; discards > maxDiscards {
				return NewData(exhaustedTag, Rec("runs", int32(runs), "discards", int32(discards), "seed", jsString(seed)))
			}
			continue
		}
		failure := evaluate(fb, property, v)
		if failure == nil {
			runs++
			continue
		}
		failureTag := "ReturnedFalse"
		if failure.(*Data).Tag == propertyErrorTag {
			failureTag = "PropertyError"
		}
		replay := "[0," + QuoteJSON(jsString(seed)) + "," + jsString(float64(attempt)) + "," +
			jsString(float64(currentSize)) + ",[]," + QuoteJSON(failureTag) + "]"
		return falsified(v, failure, runs+1, discards, replay)
	}
	return NewData(passedTag, Rec("runs", int32(runs), "discards", int32(discards)))
}

func falsified(v, failure Value, runs, discards int, replay string) Value {
	return NewData(falsifiedTag, Rec(
		"initialInput", v, "shrunkInput", v, "failure", failure, "runs", int32(runs),
		"discards", int32(discards), "shrinks", int32(0), "replay", replay,
	))
}

// replayCheck reruns the attempt a replay token records.
func replayCheck(fb *Fiber, replay string, property Value, each arbitrary) Value {
	data, reason := parseJSON(replay)
	parts, ok := data.([]Value)
	if reason != "" || !ok || len(parts) != 6 {
		panic(Defect{"Error: Invalid Arbitrary replay token"})
	}
	seed := jsNumber(parts[1]).(float64)
	v, ok := each(fb, attemptRandom(seed, int(parts[2].(float64))), int(parts[3].(float64)))
	if !ok {
		return NewData(replayMismatchTag, "AttemptDiscarded")
	}
	failure := evaluate(fb, property, v)
	switch {
	case failure == nil:
		return NewData(replayMismatchTag, "PropertyPassed")
	case (failure.(*Data).Tag == propertyErrorTag) != (parts[5] == "PropertyError"):
		return NewData(replayMismatchTag, "ShrinkPassed")
	case len(parts[4].([]Value)) > 0:
		// no failure is shrunk, so no recorded shrink path exists here
		return NewData(replayMismatchTag, "ShrinkPathUnavailable")
	}
	return falsified(v, failure, 1, 0, replay)
}

// ArbitraryFormatCheckFailure is `Arbitrary.format_check_failure`: the
// report of a failed check, `None` when it passed.
//
// shortcut: values are written by Format, not Effect's Formatter.format with two spaces; port the Formatter if a program prints a report of a falsified check
func ArbitraryFormatCheckFailure(result Value) Value {
	d := result.(*Data)
	switch d.Tag {
	case falsifiedTag:
		f := record(d.Fields[0])
		failure := "Failure: returned false"
		if e := f.Get("failure").(*Data); e.Tag == propertyErrorTag {
			failure = "Failure: " + Format(e.Fields[0])
		}
		return Some(strings.Join([]string{
			"Property falsified after " + jsString(f.Get("runs")) + " run(s) and " + jsString(f.Get("shrinks")) + " shrink(s)",
			"Shrunk input: " + Format(f.Get("shrunkInput")),
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
