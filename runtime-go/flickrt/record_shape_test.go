package flickrt

import (
	"math"
	"reflect"
	"testing"
)

// A structural record shape keeps its labels in canonical order: array-index
// keys first in ascending numeric order, then all other keys in UTF-16 order.
func TestARecordOfAShapeKeepsItsLabelsInCanonicalOrder(t *testing.T) {
	cases := []struct {
		labels []string
		keys   []string
		vals   []Value
	}{
		{[]string{"a"}, []string{"a"}, []Value{0}},
		{[]string{"b", "a"}, []string{"a", "b"}, []Value{1, 0}},
		{[]string{"b", "10", "a", "2"}, []string{"2", "10", "a", "b"}, []Value{3, 1, 2, 0}},
		{[]string{"1", "0"}, []string{"0", "1"}, []Value{1, 0}},
		{[]string{"01", "1", "-1", "1.5"}, []string{"1", "-1", "01", "1.5"}, []Value{1, 2, 0, 3}},
		{
			[]string{"i", "h", "g", "f", "e", "d", "c", "b", "a", "0"},
			[]string{"0", "a", "b", "c", "d", "e", "f", "g", "h", "i"},
			[]Value{9, 8, 7, 6, 5, 4, 3, 2, 1, 0},
		},
	}
	for _, c := range cases {
		vals := make([]Value, len(c.labels))
		for i := range vals {
			vals[i] = i
		}
		got := record(Shaped(ShapeOf(c.labels...), vals...))
		if !reflect.DeepEqual(got.keys, c.keys) || !reflect.DeepEqual(got.vals, c.vals) {
			t.Errorf("%v: %v %v, want %v %v", c.labels, got.keys, got.vals, c.keys, c.vals)
		}
	}
}

func TestARecordOfAShapeIsTheRecordOfItsLabelsAndValues(t *testing.T) {
	literals := [][]string{
		{"a"},
		{"b", "a"},
		{"b", "10", "a", "2"},
		{"a", "b", "c", "d", "e"},
		{"i", "h", "g", "f", "e", "d", "c", "b", "a", "0"},
	}
	for _, labels := range literals {
		shape := ShapeOf(labels...)
		vals, fields := make([]Value, len(labels)), []Value{}
		for i, label := range labels {
			vals[i] = int32(i)
			fields = append(fields, label, int32(i))
		}
		want := record(Rec(fields...))
		wantKeys := append([]string{}, want.keys...)
		// canonical shape has the same fields and values, in canonical order
		same := func(when string) {
			got := record(Shaped(shape, vals...))
			if len(got.keys) != len(wantKeys) {
				t.Errorf("%v %s: len %d, want %d", labels, when, len(got.keys), len(wantKeys))
			}
			for i, k := range got.keys {
				if i > 0 && !canonicalKeyLess(got.keys[i-1], k) {
					t.Errorf("%v %s: keys not canonical: %v", labels, when, got.keys)
				}
				if got.vals[i] != want.Get(k) {
					t.Errorf("%v %s: %s=%v, want %v", labels, when, k, got.vals[i], want.Get(k))
				}
			}
		}
		same("made")
		// records that grow from one of the shape leave its labels as they were
		first := Shaped(shape, vals...)
		grown := record(Update(first, Rec("zz", "new", "1", "one")))
		if len(grown.keys) != len(labels)+2 || grown.Get("zz") != "new" || grown.Get(labels[0]) != int32(0) {
			t.Errorf("%v grown: %v %v", labels, grown.keys, grown.vals)
		}
		record(first).set("yy", "new")
		record(first).set(labels[0], "set")
		same("after other records grew from it")
	}
}

// `{ r with … }` gives the record an update by a record gives, and makes
// nothing but that record.
func TestARecordWithNewValuesIsOneRecord(t *testing.T) {
	base := Rec("b", "B", "10", "ten", "a", "A", "2", "two")
	cases := []struct {
		labels []string
		vals   []Value
		keys   []string
		want   []Value
	}{
		{[]string{"a"}, []Value{"x"}, []string{"2", "10", "b", "a"}, []Value{"two", "ten", "B", "x"}},
		// the labels of the update in another order than the record keeps them
		{[]string{"a", "2", "b"}, []Value{"x", "y", "z"}, []string{"2", "10", "b", "a"}, []Value{"y", "ten", "z", "x"}},
		// labels the record has not: added as an object of JavaScript adds keys
		{[]string{"zz", "a", "1"}, []Value{"x", "y", "z"}, []string{"1", "2", "10", "b", "a", "zz"}, []Value{"z", "two", "ten", "B", "y", "x"}},
	}
	for _, c := range cases {
		got := record(With(base, ShapeOf(c.labels...), c.vals...))
		if !reflect.DeepEqual(got.keys, c.keys) || !reflect.DeepEqual(got.vals, c.want) {
			t.Errorf("with %v: %v %v, want %v %v", c.labels, got.keys, got.vals, c.keys, c.want)
		}
	}
	if got := record(base); !reflect.DeepEqual(got.vals, []Value{"two", "ten", "B", "A"}) {
		t.Errorf("the record itself changed: %v", got.vals)
	}
	shape, x, y := ShapeOf("a", "2"), Value("x"), Value("y")
	if n := testing.AllocsPerRun(100, func() { With(base, shape, x, y) }); n != 1 {
		t.Errorf("%v allocations, want 1", n)
	}
}

// The record that lies in a value of a constructor: its header over the
// values beside it in canonical label order, filled from any record of the same labels.
func TestARecordOverValuesOfItsOwner(t *testing.T) {
	shape := ShapeOf("h", "name", "w")
	owner := struct {
		rec Record
		f   [3]Value
	}{f: [3]Value{int32(2), "n", int32(1)}}
	owner.rec.Over(shape, owner.f[:])
	if !reflect.DeepEqual(owner.rec.keys, []string{"h", "name", "w"}) || Field(&owner.rec, "h") != int32(2) {
		t.Errorf("%v %v", owner.rec.keys, owner.rec.vals)
	}
	// its values are the owner's: one set there is the record's
	owner.f[2] = int32(7)
	if Field(&owner.rec, "w") != int32(7) {
		t.Errorf("w is %v, want 7", Field(&owner.rec, "w"))
	}
	var into [3]Value
	shape.Values(Rec("name", "x", "w", int32(5), "h", int32(6)), into[:])
	if into != [3]Value{int32(6), "x", int32(5)} {
		t.Errorf("from a record in another order: %v", into)
	}
	shape.Values(&owner.rec, into[:])
	if into != [3]Value{int32(2), "n", int32(7)} {
		t.Errorf("from a record of the shape: %v", into)
	}
	// a constructor applied to nothing, as the runtime applies one to learn its tag
	var none [3]Value
	shape.Values(nil, none[:])
	if none != [3]Value{} {
		t.Errorf("from no record: %v", none)
	}
	if n := testing.AllocsPerRun(100, func() { shape.Values(&owner.rec, into[:]) }); n != 0 {
		t.Errorf("%v allocations, want 0", n)
	}
}

func TestStructuralRecordOperationsCanonicalizeOrderWhileDataMapsKeepOrder(t *testing.T) {
	ext := record(RecordExtend(nil, nil, "a", int32(2), Shaped(ShapeOf("z"), int32(1))))
	if !reflect.DeepEqual(ext.keys, []string{"a", "z"}) {
		t.Errorf("RecordExtend keys = %v, want [a z]", ext.keys)
	}
	merged := record(RecordMergeDisjoint(nil, Shaped(ShapeOf("z"), int32(1)), Shaped(ShapeOf("a"), int32(2))))
	if !reflect.DeepEqual(merged.keys, []string{"a", "z"}) {
		t.Errorf("RecordMergeDisjoint keys = %v, want [a z]", merged.keys)
	}
	proj := record(RecordProject([]Value{"z", "a"}, Rec("z", int32(1), "m", int32(3), "a", int32(2))))
	if !reflect.DeepEqual(proj.keys, []string{"a", "z"}) {
		t.Errorf("RecordProject keys = %v, want [a z]", proj.keys)
	}
	rem := record(RecordRemove(nil, "m", Rec("z", int32(1), "m", int32(3), "a", int32(2))))
	if !reflect.DeepEqual(rem.keys, []string{"a", "z"}) {
		t.Errorf("RecordRemove keys = %v, want [a z]", rem.keys)
	}
	// SchemaRecord sorts unsorted field lists canonically
	s := SchemaRecord("z", SchemaInt, "a", SchemaString)
	enc := JsonStringify(CodecEncode(nil, SchemaJsonCodec(s), Shaped(ShapeOf("z", "a"), int32(1), "hi")))
	if enc != `{"a":"hi","z":1}` {
		t.Errorf("SchemaRecord json = %v, want {\"a\":\"hi\",\"z\":1}", enc)
	}
	// CodecBlock decodes into canonical order while preserving wire encoding order
	block := CodecBlock("Pair", []Value{
		"z", DslField("wire_z", SchemaJsonCodec(SchemaInt)),
		"a", DslField("wire_a", SchemaJsonCodec(SchemaString)),
	})
	dec := record(CodecDecode(nil, block, Rec("wire_z", float64(1), "wire_a", "hi")))
	if !reflect.DeepEqual(dec.keys, []string{"a", "z"}) {
		t.Errorf("CodecBlock decoded keys = %v, want [a z]", dec.keys)
	}
	wire := record(CodecEncode(nil, block, dec))
	if !reflect.DeepEqual(wire.keys, []string{"wire_z", "wire_a"}) {
		t.Errorf("CodecBlock wire keys = %v, want [wire_z wire_a]", wire.keys)
	}
	// StructCombiner and StructReducer emit canonical keys
	fb := &Fiber{}
	sumRed := reducer(func(_ *Fiber, a, b Value) Value { return a.(float64) + b.(float64) }, float64(0))
	strRed := reducer(func(_ *Fiber, a, b Value) Value { return a.(string) + b.(string) }, "")
	red := StructReducer(nil, Rec("z", sumRed, "a", strRed))
	init := record(ReducerInitialValue(red))
	if !reflect.DeepEqual(init.keys, []string{"a", "z"}) {
		t.Errorf("StructReducer initial keys = %v, want [a z]", init.keys)
	}
	combined := record(ReducerCombineAll(fb, red, []Value{Rec("z", float64(1), "a", "x"), Rec("a", "y", "z", float64(2))}))
	if !reflect.DeepEqual(combined.keys, []string{"a", "z"}) {
		t.Errorf("StructReducer combined keys = %v, want [a z]", combined.keys)
	}
	// Json and ReadonlyRecord keep insertion order
	parsed := record(value(JsonParse(`{"z":1,"a":2}`)))
	if !reflect.DeepEqual(parsed.keys, []string{"z", "a"}) {
		t.Errorf("JsonParse keys = %v, want [z a]", parsed.keys)
	}
	dict := record(RecordFromEntries([]Value{[]Value{"z", int32(1)}, []Value{"a", int32(2)}}))
	if !reflect.DeepEqual(dict.keys, []string{"z", "a"}) {
		t.Errorf("RecordFromEntries keys = %v, want [z a]", dict.keys)
	}
}

func TestHashMapOverwritesKeyUnconditionallyAndPreservesNegZero(t *testing.T) {
	byID := &Witness{
		eq: Curry2(func(_ *Fiber, a, b Value) Value {
			return Field(a, "id") == Field(b, "id")
		}),
		hash: Fn(func(_ *Fiber, _ Value) Value { return int32(7) }),
	}
	k1 := Shaped(ShapeOf("id", "tag"), int32(1), "old")
	k2 := Shaped(ShapeOf("id", "tag"), int32(1), "new")
	other := Shaped(ShapeOf("id", "tag"), int32(2), "other")
	v1 := Shaped(ShapeOf("a"), int32(1))
	v2 := Shaped(ShapeOf("a"), int32(1))

	m0 := HashMapInsert(nil, other, Shaped(ShapeOf("a"), int32(9)), HashMapInsert(nil, k1, v1, HashMapWith(byID)))
	mOver := HashMapInsert(nil, k2, v2, m0)
	entries := HashMapToList(mOver).([]Value)
	if len(entries) != 2 || Field(entries[0].([]Value)[0], "tag") != "new" {
		t.Errorf("HashMapInsert did not replace equal custom key: %v", entries)
	}

	built := HashMapFromList(nil, byID, []Value{[]Value{k1, v1}, []Value{other, Shaped(ShapeOf("a"), int32(9))}, []Value{k2, v2}})
	bEntries := HashMapToList(built).([]Value)
	if len(bEntries) != 2 || Field(bEntries[0].([]Value)[0], "tag") != "new" {
		t.Errorf("HashMapFromList did not replace equal custom key: %v", bEntries)
	}

	strW := stringWitness()
	negZero := math.Copysign(0, -1)
	zm := HashMapInsert(nil, "z", negZero, HashMapInsert(nil, "z", float64(0), HashMapWith(strW)))
	got := HashMapGet(nil, "z", zm)
	opt, ok := got.(*Data)
	if !ok || opt.Tag != "Some" {
		t.Fatalf("expected Some(-0.0), got %#v", got)
	}
	if f, ok := opt.in[0].(float64); !ok || f != 0 || !math.Signbit(f) {
		t.Errorf("HashMapInsert conflated -0.0 with 0.0: got %#v (signbit=%v)", opt.in[0], ok && math.Signbit(f))
	}
}

func TestFilterEqualsStrictUsesStructuralSameValue(t *testing.T) {
	isOk := func(v Value) bool {
		d, ok := v.(*Data)
		return ok && d.Tag == "Success"
	}
	// Records in different orders and tuples compare structurally without panicking on []Value
	r1 := Rec("z", int32(1), "a", "hi")
	r2 := Rec("a", "hi", "z", int32(1))
	if !isOk(FilterEqualsStrict(r1, r2)) {
		t.Errorf("FilterEqualsStrict failed on equal records in different orders")
	}
	t1 := []Value{int32(1), r1}
	t2 := []Value{int32(1), r2}
	if !isOk(FilterEqualsStrict(t1, t2)) {
		t.Errorf("FilterEqualsStrict failed on equal tuples")
	}
	if isOk(FilterEqualsStrict([]Value{int32(1)}, []Value{int32(2)})) {
		t.Errorf("FilterEqualsStrict accepted distinct tuples")
	}
	// Distinguishes +0.0 from -0.0, equates NaN with NaN
	negZero := math.Copysign(0, -1)
	if isOk(FilterEqualsStrict(float64(0), negZero)) {
		t.Errorf("FilterEqualsStrict conflated 0.0 and -0.0")
	}
	if !isOk(FilterEqualsStrict(negZero, negZero)) {
		t.Errorf("FilterEqualsStrict rejected -0.0 == -0.0")
	}
	if !isOk(FilterEqualsStrict(math.NaN(), math.NaN())) {
		t.Errorf("FilterEqualsStrict rejected NaN == NaN")
	}
	if !isOk(FilterEquals(float64(0), negZero)) {
		t.Errorf("FilterEquals should still equate 0.0 and -0.0")
	}
	// Mutable cells keep reference identity
	c1 := RefMake(int32(1))
	c2 := RefMake(int32(1))
	if isOk(FilterEqualsStrict(c1, c2)) {
		t.Errorf("FilterEqualsStrict conflated distinct Ref cells")
	}
	if !isOk(FilterEqualsStrict(c1, c1)) {
		t.Errorf("FilterEqualsStrict rejected identical Ref cell")
	}
	// Chunks use strictZero comparison
	ch0 := ChunkFromList([]Value{float64(0)})
	chNeg0 := ChunkFromList([]Value{negZero})
	if isOk(FilterEqualsStrict(ch0, chNeg0)) {
		t.Errorf("FilterEqualsStrict conflated Chunk(0.0) and Chunk(-0.0)")
	}
	if !isOk(FilterEqualsStrict(ch0, ChunkFromList([]Value{float64(0)}))) {
		t.Errorf("FilterEqualsStrict rejected equal Chunks")
	}
}

func TestGADTDictFieldsSkippedInHashAndEquality(t *testing.T) {
	d1 := NewDict()
	d1.Set("show", Fn(func(_ *Fiber, _ Value) Value { return "a" }))
	d2 := NewDict()
	d2.Set("show", Fn(func(_ *Fiber, _ Value) Value { return "b" }))

	g1 := &Data{Tag: "App.Box", n: 2, in: [2]Value{int32(1), d1}}
	g2 := &Data{Tag: "App.Box", n: 2, in: [2]Value{int32(1), d2}}
	plain := &Data{Tag: "App.Box", n: 1, in: [2]Value{int32(1)}}

	if !Equals(g1, g2) || !SameValue(g1, g2) {
		t.Errorf("GADT values with distinct *Dict fields should be equal")
	}
	if Hash(g1) != Hash(g2) {
		t.Errorf("Hash(g1)=%v != Hash(g2)=%v", Hash(g1), Hash(g2))
	}
	if Hash(g1) != Hash(plain) {
		t.Errorf("Hash(g1)=%v != Hash(plain)=%v", Hash(g1), Hash(plain))
	}
	negZero := math.Copysign(0, -1)
	gz1 := &Data{Tag: "App.Box", n: 2, in: [2]Value{float64(0), d1}}
	gz2 := &Data{Tag: "App.Box", n: 2, in: [2]Value{negZero, d2}}
	if !Equals(gz1, gz2) {
		t.Errorf("Equals should equate GADT(0.0) and GADT(-0.0)")
	}
	if SameValue(gz1, gz2) {
		t.Errorf("SameValue should distinguish GADT(0.0) and GADT(-0.0)")
	}
}

func TestTxHashMapOverwritesKeyUnconditionallyAndPreservesNegZero(t *testing.T) {
	fb := &Fiber{}
	byID := &Witness{
		eq: Curry2(func(_ *Fiber, a, b Value) Value {
			return Field(a, "id") == Field(b, "id")
		}),
		hash: Fn(func(_ *Fiber, _ Value) Value { return int32(7) }),
	}
	k1 := Shaped(ShapeOf("id", "tag"), int32(1), "old")
	k2 := Shaped(ShapeOf("id", "tag"), int32(1), "new")
	shared := Shaped(ShapeOf("a"), int32(1))

	keyTag := func(m Value) Value {
		entries := TxHashMapEntries(fb, m).([]Value)
		if len(entries) != 1 {
			t.Fatalf("expected 1 entry, got %d", len(entries))
		}
		return Field(entries[0].([]Value)[0], "tag")
	}

	built := TxHashMapMake(fb, byID, []Value{[]Value{k1, shared}, []Value{k2, shared}})
	if keyTag(built) != "new" {
		t.Errorf("TxHashMapMake did not replace equal custom key")
	}

	m := TxHashMapMake(fb, byID, []Value{[]Value{k1, shared}})
	TxHashMapSet(fb, k2, shared, m)
	if keyTag(m) != "new" {
		t.Errorf("TxHashMapSet did not replace equal custom key")
	}

	m2 := TxHashMapMake(fb, byID, []Value{[]Value{k1, shared}})
	TxHashMapModify(fb, k2, Fn(func(_ *Fiber, _ Value) Value { return shared }), m2)
	if keyTag(m2) != "new" {
		t.Errorf("TxHashMapModify did not replace equal custom key")
	}

	m3 := TxHashMapMake(fb, byID, []Value{[]Value{k1, shared}})
	TxHashMapModifyAt(fb, k2, Fn(func(_ *Fiber, _ Value) Value { return Some(shared) }), m3)
	if keyTag(m3) != "new" {
		t.Errorf("TxHashMapModifyAt did not replace equal custom key")
	}

	m4 := TxHashMapMake(fb, byID, []Value{[]Value{k1, shared}})
	TxHashMapUnion(fb, HashMapFromList(fb, byID, []Value{[]Value{k2, shared}}), m4)
	if keyTag(m4) != "new" {
		t.Errorf("TxHashMapUnion did not replace equal custom key")
	}

	m5 := TxHashMapMake(fb, byID, []Value{[]Value{k1, shared}})
	TxHashMapSetMany(fb, []Value{[]Value{k2, shared}}, m5)
	if keyTag(m5) != "new" {
		t.Errorf("TxHashMapSetMany did not replace equal custom key")
	}

	// Overwriting 0.0 with -0.0 preserves -0.0
	strW := stringWitness()
	negZero := math.Copysign(0, -1)
	zm := TxHashMapMake(fb, strW, []Value{[]Value{"z", float64(0)}})
	TxHashMapSet(fb, "z", negZero, zm)
	opt, ok := TxHashMapGet(fb, "z", zm).(*Data)
	if !ok || opt.Tag != "Some" {
		t.Fatalf("expected Some(-0.0), got %#v", opt)
	}
	if f, ok := opt.in[0].(float64); !ok || f != 0 || !math.Signbit(f) {
		t.Errorf("TxHashMapSet conflated -0.0 with 0.0: got %#v", opt.in[0])
	}
}
