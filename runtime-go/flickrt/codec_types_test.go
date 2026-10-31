package flickrt

import "testing"

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
