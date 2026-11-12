package flickrt

import (
	"testing"
)

func TestDurationCodecs(t *testing.T) {
	roundTrip(t, CodecDurationString, "1500 millis")
	roundTrip(t, CodecDurationNanos, "2500")
	if _, issues := codecOf(CodecDurationString).decode(rootFiber(), "-1 seconds"); len(issues) != 1 ||
		issues[0].message != "expected a non-negative duration, got an object" {
		t.Fatalf("a negative duration gave %v", issues)
	}
	if _, issues := codecOf(CodecDurationNanos).encode(rootFiber(), DurationInfinity); issues == nil {
		t.Fatal("an infinite duration had nanoseconds")
	}
}

func TestChunkSchema(t *testing.T) {
	s := SchemaChunk(SchemaString)
	if r := SchemaValidate(rootFiber(), s, ChunkFromList([]Value{"a"})); !isOk(r) {
		t.Fatalf("a chunk of strings gave %v", r)
	}
	if r := SchemaValidate(rootFiber(), s, []Value{"a"}); isOk(r) {
		t.Fatal("a list passed as a chunk")
	}
	if c := roundTrip(t, codecOf(s.(*Schema).jsonOf()), []Value{"a", "b"}); ChunkSize(c) != int32(2) {
		t.Fatalf("decoded %v", c)
	}
}

func TestPair(t *testing.T) {
	p := &pair{codecOf(CodecString), codecOf(CodecNumberFinite)}
	roundTrip(t, p, []Value{"a", 1.0})
	if _, issues := p.decode(rootFiber(), []Value{"a", 1.0, 2.0}); len(issues) != 1 || issues[0] != (issue{"[2]", "an unknown key"}) {
		t.Fatalf("three elements gave %v", issues)
	}
}

func TestZonedDateTime(t *testing.T) {
	d, ok := makeZonedFromString("2024-07-01T10:00:00Z[Europe/Paris]")
	if !ok || d.formatIsoZoned() != "2024-07-01T12:00:00.000+02:00[Europe/Paris]" {
		t.Fatalf("decoded %v", d)
	}
	roundTrip(t, CodecDateTimeZoned, "2024-01-01T10:00:00.000-05:30")
	if _, ok := makeZonedFromString("2024-01-01T10:00:00Z"); ok {
		t.Fatal("a date-time without a zone was zoned")
	}
}

func TestGraphJson(t *testing.T) {
	c := GraphJson("directed", CodecString, CodecNumberFinite)
	in := Rec("type", "directed", "nodes", []Value{Rec("index", 0.0, "data", "a"), Rec("index", 2.0, "data", "b")},
		"edges", []Value{Rec("index", 0.0, "source", 2.0, "target", 0.0, "data", 1.0)})
	roundTrip(t, c, in)
	bad := Rec("type", "directed", "nodes", []Value{}, "edges", []Value{Rec("index", 0.0, "source", 0.0, "target", 0.0, "data", 1.0)})
	if _, issues := codecOf(c).decode(rootFiber(), bad); len(issues) != 1 || issues[0].message != "an invalid value" {
		t.Fatalf("an edge to no node gave %v", issues)
	}
}
