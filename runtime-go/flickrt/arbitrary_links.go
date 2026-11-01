package flickrt

import (
	"math"
	"math/big"
	"sync"
)

// The declarations Effect generates through a link to another schema, as
// far as the program's types reach them: JSON values for Unknown,
// Duration, Chunk, and DateTime with its zones. A link is a node of the
// declaration, with the declaration's checks, over the node of what Effect
// generates (Schema.ts's toCodecArbitrary and toCodecJson links,
// internal/arbitrary/schema.ts's `jsonSchema`).

// linkNode is a declaration whose values are `decode` of `target`'s.
func linkNode(target *arbAST, decode func(Value) Value, params ...*arbAST) *arbAST {
	return &arbAST{tag: astLink, members: append([]*arbAST{target}, params...), decode: decode}
}

func checkFilter(c Value) arbFilter { k := c.(*check); return arbFilter{k.arb, k.test} }

// intBetween is Effect's `Int.check(isBetween({ minimum, maximum }))`.
func intBetween(minimum, maximum float64) *arbAST {
	between := CheckBetween(Rec("minimum", minimum, "maximum", maximum, "exclusiveMinimum", false, "exclusiveMaximum", false))
	return astNumberNode.withChecks(checkFilter(CheckInt), checkFilter(between))
}

func literalUnion(vs ...string) *arbAST {
	members := make([]*arbAST, len(vs))
	for i, v := range vs {
		members[i] = literalNode(v)
	}
	return &arbAST{tag: astUnion, members: members}
}

func floatOf(v Value) float64 { return v.(float64) }

// durationNode is Effect's Duration, linked by its JSON codec to a union
// of the infinities, nanoseconds and milliseconds.
func durationNode() *arbAST {
	infinite := func(tag string, d Value) *arbAST {
		return structNode(tag, nil, func([]Value) Value { return d })
	}
	return linkNode(&arbAST{tag: astUnion, members: []*arbAST{
		infinite("Infinity", DurationInfinity),
		infinite("NegativeInfinity", &Duration{tag: durationNegativeInfinite}),
		structNode("Nanos", []*arbAST{astBigIntNode}, func(vs []Value) Value { return fromNanos(vs[0].(*big.Int)) }),
		structNode("Millis", []*arbAST{astNumberNode.withChecks(checkFilter(CheckInt))},
			func(vs []Value) Value { return fromMillis(floatOf(vs[0])) }),
	}}, Identity)
}

func chunkNode(item *arbAST) *arbAST {
	return linkNode(&arbAST{tag: astArrays, rest: item}, ChunkFromList, item)
}

const (
	dayMillis       = 8_640_000_000_000_000
	zoneSlackMillis = 14 * 60 * 60 * 1000
)

var namedZones = []string{"UTC", "Europe/London", "America/New_York", "Asia/Tokyo", "Australia/Sydney"}

func namedZone(v Value) Value {
	z, ok := zoneNamed(v.(string))
	if !ok {
		panic(rangeError("Invalid time zone: " + v.(string)))
	}
	return z
}

func zoneOf(v Value) Value {
	if n, ok := v.(float64); ok {
		return &TimeZone{offset: n}
	}
	return namedZone(v)
}

// zones is Effect's `timeZoneArbitrarySchema`: an offset or a named zone.
func zones() *arbAST {
	return &arbAST{tag: astUnion, members: []*arbAST{intBetween(-12*60*60*1000, zoneSlackMillis), literalUnion(namedZones...)}}
}

func utcNode() *arbAST {
	return linkNode(intBetween(-dayMillis, dayMillis), func(v Value) Value { return &DateTime{ms: floatOf(v)} })
}

func zonedNode() *arbAST {
	epoch := intBetween(-dayMillis+zoneSlackMillis, dayMillis-zoneSlackMillis)
	return linkNode(&arbAST{tag: astObjects, props: []*arbAST{epoch, zones()}, names: []string{"epochMilliseconds", "timeZone"}, make: func(vs []Value) Value {
		return &DateTime{floatOf(vs[0]), zoneOf(vs[1]).(*TimeZone)}
	}}, Identity)
}

// jsonNode is Effect's Json declaration, linked to null, finite numbers,
// booleans, strings, and arrays and string-keyed records of JSON values.
func (b *arbASTs) jsonNode() *arbAST {
	if b.json == nil {
		union := &arbAST{tag: astUnion}
		suspend := func() *arbAST { return &arbAST{tag: astSuspend, thunk: func() *arbAST { return union }} }
		union.members = []*arbAST{
			literalNode(JsonNull),
			astNumberNode.withChecks(checkFilter(CheckFinite)),
			astBooleanNode,
			astStringNode,
			{tag: astArrays, rest: suspend()},
			{tag: astRecord, members: []*arbAST{astStringNode, suspend()}},
		}
		b.json = linkNode(union, Identity)
	}
	return b.json
}

// arbLinks are the declared types of codecs, by codec: what the decoded
// side of `Codec.to_schema` of one generates.
var arbLinks sync.Map // codec → func(*arbASTs) *arbAST

func linkCodec(c Value, node func(*arbASTs) *arbAST) Value {
	arbLinks.Store(c, node)
	return c
}

func init() {
	duration := func(*arbASTs) *arbAST { return durationNode() }
	linkCodec(CodecDurationMillis, duration)
	linkCodec(CodecDurationNanos, duration)
	linkCodec(CodecDurationString, func(*arbASTs) *arbAST { return durationNode().withChecks(checkFilter(nonNegativeDuration)) })
	utc := func(*arbASTs) *arbAST { return utcNode() }
	linkCodec(CodecDateTimeUtc, utc)
	linkCodec(CodecDateTimeUtcMillis, utc)
	linkCodec(CodecDateTimeZoned, func(*arbASTs) *arbAST { return zonedNode() })
	linkCodec(CodecTimeZone, func(*arbASTs) *arbAST { return linkNode(zones(), zoneOf) })
	linkCodec(CodecTimeZoneNamed, func(*arbASTs) *arbAST { return linkNode(literalUnion(namedZones...), namedZone) })
	linkCodec(CodecTimeZoneOffset, func(*arbASTs) *arbAST {
		return linkNode(astNumberNode.withChecks(checkFilter(CheckInt)), zoneOf)
	})
}

// compileRecord is Effect's `compileObjects` of a record without
// properties and with one index signature: keys drawn from its key's
// node, each new, each with a value of its value's node.
func (c *arbCompiler) compileRecord(ast *arbAST, path []any) *arbGen {
	value := c.recur(ast.members[1], at(path, "index-0-value"), nil)
	key := c.recur(ast.members[0], at(path, "index-0-key"), nil)
	return compiledGen([]*arbGen{key, value}, func() float64 { return 0 }, func(st *genState) *sample {
		upper := math.Max(0, st.size)
		st.randomInt(0, 0) // the optional properties, of which there are none
		indexCost := key.minCost + value.minCost
		affordable := upper
		switch {
		case indexCost == infinity:
			affordable = 0
		case indexCost != 0:
			affordable = math.Min(upper, math.Floor(*st.budget/indexCost))
		}
		if affordable < 0 {
			return nil
		}
		count := int(st.randomLength(0, affordable))
		if reserved := float64(count) * indexCost; count > 0 && reserved > *st.budget || count == 0 && *st.budget < 0 {
			return nil
		}
		var keys []*sample
		var vals []*sample
		taken := func(k Value) bool {
			for _, other := range keys {
				if other.value == k {
					return true
				}
			}
			return false
		}
		for position := range count {
			future := float64(count-position-1) * indexCost
			if indexCost+future > *st.budget {
				return nil
			}
			st.index(1)
			budget, keyState := *st.budget, st
			if st.size < float64(count) {
				keyState = st.withSize(float64(count))
			}
			var k *sample
			for retries := 0; ; retries++ {
				if k = reserving(key, keyState, value.minCost+future); k == nil {
					return nil
				}
				if !taken(k.value) {
					break
				}
				if retries >= 10 {
					return nil
				}
				*st.budget = budget
			}
			v := reserving(value, st, future)
			if v == nil {
				return nil
			}
			keys, vals = append(keys, k), append(vals, v)
		}
		return entriesSample(keys, vals, st.shrinks)
	})
}

// recordEntry is a removable entry of a record: its key, and the retained
// samples of its key and value.
type recordEntry struct {
	key        string
	keySample  *retained
	valueEntry *retained
}

func entriesRecord(keys []string, vals []Value) Value {
	out := &Record{}
	for i, k := range keys {
		out = out.set(k, vals[i])
	}
	return out
}

// entriesSample is Effect's `objectSample` of removable entries: shrunk by
// removing an entry, then by shrinking a value, then a key to one no
// other entry has.
func entriesSample(keySamples, valueSamples []*sample, shrinks bool) *sample {
	keys, vals := make([]string, len(keySamples)), values(valueSamples)
	for i, k := range keySamples {
		keys[i] = k.value.(string)
	}
	if !shrinks {
		return &sample{entriesRecord(keys, vals), nil}
	}
	var rebuild func(entries []recordEntry, valueStart, keyStart int) *sample
	rebuild = func(entries []recordEntry, valueStart, keyStart int) *sample {
		var pulls []shrinkPull
		if len(entries) > 0 {
			removals := make([]int, len(entries))
			for i := range removals {
				removals[i] = i
			}
			pulls = append(pulls, pullEach(removals, func(i int) *sample {
				return rebuild(append(append([]recordEntry(nil), entries[:i]...), entries[i+1:]...), 0, 0)
			}))
		}
		for i := valueStart; i < len(entries); i++ {
			e := entries[i]
			if e.valueEntry.shrinks == nil {
				continue
			}
			pulls = append(pulls, mapRetained(e.valueEntry.shrinks(), func(r *retained) *sample {
				return rebuild(replaceAt(entries, i, recordEntry{e.key, e.keySample, r}), i, 0)
			}))
		}
		for i := keyStart; i < len(entries); i++ {
			e := entries[i]
			if e.keySample.shrinks == nil {
				continue
			}
			unique := func(k Value) bool {
				for j, other := range entries {
					if j != i && other.key == k {
						return false
					}
				}
				return true
			}
			filtered := filterSample(fromRetained(e.keySample), unique)
			if filtered == nil || filtered.shrinks == nil {
				continue
			}
			pulls = append(pulls, mapPull(filtered.shrinks, func(s *sample) *sample {
				return rebuild(replaceAt(entries, i, recordEntry{s.value.(string), retain(s), e.valueEntry}), len(entries), i)
			}))
		}
		keys, vals := make([]string, len(entries)), make([]Value, len(entries))
		for i, e := range entries {
			keys[i], vals[i] = e.key, e.valueEntry.value
		}
		out := &sample{entriesRecord(keys, vals), nil}
		if len(pulls) > 0 {
			out.shrinks = concatPulls(pulls)
		}
		return out
	}
	return lazyShrinks(entriesRecord(keys, vals), func() shrinkPull {
		entries := make([]recordEntry, len(keys))
		for i, k := range keys {
			entries[i] = recordEntry{k, retain(keySamples[i]), retain(valueSamples[i])}
		}
		return rebuild(entries, 0, 0).shrinks
	})
}
