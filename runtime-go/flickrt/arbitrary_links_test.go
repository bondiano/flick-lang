package flickrt

import "testing"

// The declarations Effect generates through a link, or cannot generate:
// testdata/arbitrary_links.txt is Effect's output on Node for the same
// schemas, seeds and properties (tools/go-arbitrary/gen.mjs).

func arbLinkCases() []arbCase {
	UseZones()
	s := ArbitrarySchema
	of := func(c Value) func() Value { return func() Value { return s(CodecToSchema(c)) } }
	isRecord := func(v Value) bool { r, ok := v.(*Record); return ok && len(r.keys) > 0 }
	isList := func(v Value) bool { _, ok := v.([]Value); return ok }
	hashInt := NewDict().Set("hash", Fn(func(_ *Fiber, n Value) Value { return n })).
		Set("$Eq", NewDict().Set("eq", Curry2(func(_ *Fiber, a, b Value) Value { return a == b })))
	zone := func(v Value) *TimeZone { return v.(*TimeZone) }
	impossible := CodecToSchema(CheckApply(CheckBetween(Rec("minimum", 9.0, "maximum", 0.0, "exclusiveMinimum", false, "exclusiveMaximum", false)),
		CodecNumberFinite))
	always := func(Value) bool { return true }
	lazy := func(schema Value) func() Value { return func() Value { return s(schema) } }
	return []arbCase{
		{name: "unknown", arb: s(SchemaUnknown), property: func(v Value) bool { return !isList(v) }},
		{name: "unknown-record", arb: s(SchemaUnknown), property: func(v Value) bool { return !isRecord(v) }},
		{name: "unknown-fields", arb: s(SchemaRecord("a", SchemaUnknown, "b", SchemaUnknown)),
			property: func(v Value) bool { return !isList(record(v).Get("a")) }},
		{name: "opaque", arb: s(SchemaOpaque("Conn", "none")), property: func(v Value) bool { _, ok := v.(string); return !ok }},
		{name: "redacted-json", arb: of(SchemaJsonCodec(SchemaRedacted(SchemaBool))), property: func(v Value) bool { return v != JsonNull }},
		{name: "duration", arb: s(SchemaDuration), property: func(v Value) bool { return toMillis(v).(float64) < 1000 }},
		{name: "duration-millis", arb: of(CodecDurationMillis), property: func(v Value) bool { return dur(v).tag != durationNanos }},
		{name: "duration-string", arb: of(CodecDurationString), property: func(v Value) bool { return dur(v).tag != durationMillis }},
		{name: "chunk", arb: s(SchemaChunk(SchemaBool)), property: func(v Value) bool { return len(v.(*Chunk).items) < 3 }},
		{name: "chunk-json", arb: of(CodecChunk(SchemaJsonCodec(SchemaString))), property: func(v Value) bool { return len(v.(*Chunk).items) < 2 }},
		{name: "utc", arb: of(CodecDateTimeUtc), property: func(v Value) bool { return dateTime(v).ms < 0 }},
		{name: "utc-millis", arb: of(CodecDateTimeUtcMillis), property: func(v Value) bool { return dateTime(v).ms > 0 }},
		{name: "zoned", arb: of(CodecDateTimeZoned), property: func(v Value) bool { return dateTime(v).zone.loc == nil }},
		{name: "zone", arb: of(CodecTimeZone), property: func(v Value) bool { return zone(v).loc == nil }},
		{name: "zone-named", arb: of(CodecTimeZoneNamed), property: func(v Value) bool { return zone(v).id != "Asia/Tokyo" }},
		{name: "zone-offset", arb: of(CodecTimeZoneOffset), property: func(v Value) bool { return zone(v).offset < 1000 }},
		{name: "hashset", arb: of(CodecHashSet(rootFiber(), hashInt, CodecInt)), property: func(Value) bool { return true }},
		{name: "hashmap", arb: of(CodecHashMap(rootFiber(), hashInt, CodecInt, CodecInt)), property: func(Value) bool { return true }},
		{name: "result-json", arb: of(SchemaJsonCodec(SchemaResult(SchemaString, SchemaBool))), property: func(Value) bool { return true }},
		{name: "path-option-list", arb: lazy(SchemaOption(SchemaList(impossible))), property: always},
		{name: "path-result", arb: lazy(SchemaResult(impossible, SchemaNumber)), property: always},
		{name: "path-record", arb: lazy(SchemaRecord("a", SchemaNumber, "b c", SchemaList(impossible))), property: always},
		{name: "path-variant", arb: lazy(SchemaAdt("Shape", []Variant{{Tag: "Shape.Dot", Name: "Dot"}, {Tag: "Shape.Circle", Name: "Circle", Fields: []Value{impossible}}})),
			property: always},
	}
}

func TestArbitraryLinksAsEffect(t *testing.T) {
	compareArbOracle(t, "testdata/arbitrary_links.txt", arbLinkCases())
}
