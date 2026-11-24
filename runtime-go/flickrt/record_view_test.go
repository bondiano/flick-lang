package flickrt

import (
	"math"
	"reflect"
	"testing"
)

// viewPoint mimics a compiler-generated closed record struct `{ x : Float, y : Float }`
// implementing RecordView and RecordEqual by value.
type viewPoint struct {
	f0 float64
	f1 float64
}

var viewPointKeys = []string{"x", "y"}

func (r viewPoint) Get(k string) Value {
	switch k {
	case "x":
		return r.f0
	case "y":
		return r.f1
	}
	return nil
}

func (r viewPoint) Lookup(k string) (Value, bool) {
	switch k {
	case "x":
		return r.f0, true
	case "y":
		return r.f1, true
	}
	return nil, false
}

func (r viewPoint) Keys() []string { return viewPointKeys }
func (r viewPoint) Len() int       { return 2 }

func (r viewPoint) At(i int) Value {
	switch i {
	case 0:
		return r.f0
	case 1:
		return r.f1
	}
	return nil
}

func (r viewPoint) EqualRecord(other Value, strictZero bool) (bool, bool) {
	q, ok := other.(viewPoint)
	if !ok {
		return false, false
	}
	return EqualFloat(r.f0, q.f0, strictZero) && EqualFloat(r.f1, q.f1, strictZero), true
}

// viewUncomparable holds a slice field (uncomparable in Go) to verify no map-key panics.
type viewUncomparable struct {
	f0 Value
	f1 string
}

var viewUncomparableKeys = []string{"items", "tag"}

func (r viewUncomparable) Get(k string) Value {
	switch k {
	case "items":
		return r.f0
	case "tag":
		return r.f1
	}
	return nil
}

func (r viewUncomparable) Lookup(k string) (Value, bool) {
	switch k {
	case "items":
		return r.f0, true
	case "tag":
		return r.f1, true
	}
	return nil, false
}

func (r viewUncomparable) Keys() []string { return viewUncomparableKeys }
func (r viewUncomparable) Len() int       { return 2 }

func (r viewUncomparable) At(i int) Value {
	switch i {
	case 0:
		return r.f0
	case 1:
		return r.f1
	}
	return nil
}

// viewTagged carries a `_tag` field to verify TagField.
type viewTagged struct{ f0 string }

func (r viewTagged) Get(k string) Value {
	if k == "_tag" {
		return r.f0
	}
	return nil
}

func (r viewTagged) Lookup(k string) (Value, bool) {
	if k == "_tag" {
		return r.f0, true
	}
	return nil, false
}

func (r viewTagged) Keys() []string { return []string{"_tag"} }
func (r viewTagged) Len() int       { return 1 }
func (r viewTagged) At(int) Value   { return r.f0 }

// viewFunc holds a closure (`Fn`, uncomparable in Go) to verify Same/identical never panic.
type viewFunc struct {
	f0 Value
	f1 int32
}

var viewFuncKeys = []string{"cb", "id"}

func (r viewFunc) Get(k string) Value {
	switch k {
	case "cb":
		return r.f0
	case "id":
		return r.f1
	}
	return nil
}

func (r viewFunc) Lookup(k string) (Value, bool) {
	switch k {
	case "cb":
		return r.f0, true
	case "id":
		return r.f1, true
	}
	return nil, false
}

func (r viewFunc) Keys() []string { return viewFuncKeys }
func (r viewFunc) Len() int       { return 2 }

func (r viewFunc) At(i int) Value {
	switch i {
	case 0:
		return r.f0
	case 1:
		return r.f1
	}
	return nil
}

func TestRecordViewFieldEqualityHashAndStructural(t *testing.T) {
	p := viewPoint{f0: 1.5, f1: 2.5}
	pSame := viewPoint{f0: 1.5, f1: 2.5}
	pDiff := viewPoint{f0: 1.5, f1: 3.0}
	rSame := Rec("y", 2.5, "x", 1.5)
	rDiff := Rec("x", 1.5, "y", 3.0)
	rExtra := Rec("x", 1.5, "y", 2.5, "z", 0.0)

	if Field(p, "x") != 1.5 || Field(p, "y") != 2.5 {
		t.Fatalf("Field on RecordView failed: x=%v y=%v", Field(p, "x"), Field(p, "y"))
	}
	if TagField(viewTagged{f0: "Custom"}) != "Custom" {
		t.Fatalf("TagField on RecordView failed: %v", TagField(viewTagged{f0: "Custom"}))
	}

	// Equality: struct vs struct, struct vs *Record, *Record vs struct
	if !Equals(p, pSame) || Equals(p, pDiff) {
		t.Errorf("Equals struct/struct failed")
	}
	if !Equals(p, rSame) || !Equals(rSame, p) {
		t.Errorf("Equals struct/*Record failed")
	}
	if Equals(p, rDiff) || Equals(rDiff, p) || Equals(p, rExtra) || Equals(rExtra, p) {
		t.Errorf("Equals distinct records returned true")
	}

	// NaN and -0.0 semantics for Equals vs SameValue
	negZero := math.Copysign(0, -1)
	pZero := viewPoint{f0: 0.0, f1: math.NaN()}
	pNeg := viewPoint{f0: negZero, f1: math.NaN()}
	rNeg := Rec("x", negZero, "y", math.NaN())
	if !Equals(pZero, pNeg) || !Equals(pZero, rNeg) || !Equals(rNeg, pZero) {
		t.Errorf("Equals should treat +0/-0 and NaN/NaN as equal")
	}
	if SameValue(pZero, pNeg) || SameValue(pZero, rNeg) || SameValue(rNeg, pZero) {
		t.Errorf("SameValue should distinguish +0 and -0")
	}
	if !SameValue(pNeg, pNeg) || !SameValue(pNeg, rNeg) || !SameValue(rNeg, pNeg) {
		t.Errorf("SameValue should equate -0/-0 and NaN/NaN")
	}

	// Hash matches *Record
	if Hash(p) != Hash(rSame) {
		t.Errorf("Hash mismatch: struct=%v *Record=%v", Hash(p), Hash(rSame))
	}

	// Structural Eq, Ord, Hash, Show
	fb := &Fiber{}
	floatEq := NewDict().Set("eq", Curry2(func(_ *Fiber, a, b Value) Value { return EqFloat(a.(float64), b.(float64)) }))
	floatOrd := NewDict().Set("compare", Curry2(func(_ *Fiber, a, b Value) Value { return CompareFloat(a.(float64), b.(float64)) })).Set("$Eq", floatEq)
	floatHash := NewDict().Set("hash", Fn(func(_ *Fiber, a Value) Value { return HashFloat(a.(float64)) })).Set("$Eq", floatEq)
	floatShow := NewDict().Set("show", Fn(func(_ *Fiber, a Value) Value { return ShowFloat(a.(float64)) }))

	eqDict := Structural("Eq", []Value{floatEq, floatEq})
	if !Bool(Apply(fb, Apply(fb, Method(fb, eqDict, "eq"), p), rSame)) {
		t.Errorf("Structural Eq failed")
	}
	ordDict := Structural("Ord", []Value{floatOrd, floatOrd})
	if Apply(fb, Apply(fb, Method(fb, ordDict, "compare"), p), pDiff).(*Data).Tag != LT.(*Data).Tag {
		t.Errorf("Structural Ord failed")
	}
	hashDict := Structural("Hash", []Value{floatHash, floatHash})
	if Apply(fb, Method(fb, hashDict, "hash"), p) != Apply(fb, Method(fb, hashDict, "hash"), rSame) {
		t.Errorf("Structural Hash mismatch")
	}
	showDict := Structural("Show", []Value{floatShow, floatShow})
	if got := Apply(fb, Method(fb, showDict, "show"), p); got != "{ x = 1.5, y = 2.5 }" {
		t.Errorf("Structural Show = %q, want %q", got, "{ x = 1.5, y = 2.5 }")
	}
}

func TestRecordViewMutationsRecordOpsSchemaCodecAndInert(t *testing.T) {
	fb := &Fiber{}
	p := viewPoint{f0: 1.5, f1: 2.5}

	// Update and With preserve original and produce updated record
	upd := record(Update(p, Rec("x", 9.0)))
	if upd.Get("x") != 9.0 || upd.Get("y") != 2.5 {
		t.Errorf("Update failed: %v %v", upd.keys, upd.vals)
	}
	withRes := record(With(p, ShapeOf("y", "z"), 8.0, "hi"))
	if withRes.Get("x") != 1.5 || withRes.Get("y") != 8.0 || withRes.Get("z") != "hi" {
		t.Errorf("With failed: %v %v", withRes.keys, withRes.vals)
	}

	// Record.* operations
	if got := RecordLabels(nil, p).([]Value); !reflect.DeepEqual(got, []Value{"x", "y"}) {
		t.Errorf("RecordLabels = %v", got)
	}
	if RecordGet(nil, "y", p) != 2.5 || RecordSize(p) != int32(2) || RecordIsEmpty(p).(bool) {
		t.Errorf("RecordGet/Size/IsEmpty failed")
	}
	if !RecordHas("x", p).(bool) || RecordHas("z", p).(bool) {
		t.Errorf("RecordHas failed")
	}
	ext := record(RecordExtend(nil, nil, "a", int32(7), p))
	if !reflect.DeepEqual(ext.keys, []string{"a", "x", "y"}) || ext.Get("a") != int32(7) {
		t.Errorf("RecordExtend = %v %v", ext.keys, ext.vals)
	}
	rem := record(RecordRemove(nil, "x", p))
	if !reflect.DeepEqual(rem.keys, []string{"y"}) || rem.Get("y") != 2.5 {
		t.Errorf("RecordRemove = %v %v", rem.keys, rem.vals)
	}
	proj := record(RecordProject([]Value{"y"}, p))
	if !reflect.DeepEqual(proj.keys, []string{"y"}) || proj.Get("y") != 2.5 {
		t.Errorf("RecordProject = %v %v", proj.keys, proj.vals)
	}
	merged := record(RecordMergeDisjoint(nil, p, viewTagged{f0: "ok"}))
	if !reflect.DeepEqual(merged.keys, []string{"_tag", "x", "y"}) || merged.Get("_tag") != "ok" {
		t.Errorf("RecordMergeDisjoint = %v %v", merged.keys, merged.vals)
	}
	if got := JsonStringify(p); got != `{"x":1.5,"y":2.5}` {
		t.Errorf("JsonStringify = %q", got)
	}

	// Schema validate / encode / decode / write
	s := SchemaRecord("x", SchemaNumber, "y", SchemaNumber)
	codec := SchemaJsonCodec(s)
	if enc := JsonStringify(CodecEncode(fb, codec, p)); enc != `{"x":1.5,"y":2.5}` {
		t.Errorf("SchemaJsonCodec encode = %q", enc)
	}
	if validated, issues := sch(s).validate(fb, p); issues != nil || !Equals(validated, p) {
		t.Errorf("Schema validate = %v, issues = %v", validated, issues)
	}
	block := CodecBlock("Point", []Value{
		"x", DslField("wx", SchemaJsonCodec(SchemaNumber)),
		"y", DslField("wy", SchemaJsonCodec(SchemaNumber)),
	})
	if enc := JsonStringify(CodecEncode(fb, block, p)); enc != `{"wx":1.5,"wy":2.5}` {
		t.Errorf("CodecBlock encode = %q", enc)
	}

	// Uncomparable value-type struct (holds []Value) must not panic in inertProblem, Equals, Hash, Inspect
	u1 := viewUncomparable{f0: []Value{int32(1), int32(2)}, f1: "a"}
	u2 := viewUncomparable{f0: []Value{int32(1), int32(2)}, f1: "a"}
	if issues := inertProblem(u1); issues != nil {
		t.Errorf("inertProblem(u1) = %v, want nil", issues)
	}
	if !Equals(u1, u2) || Hash(u1) != Hash(u2) {
		t.Errorf("uncomparable RecordView equality/hash failed")
	}
	if s := formatSpaced(u1, 0); s == "" {
		t.Errorf("formatSpaced returned empty")
	}

	// Service env accepts value-type RecordView implementation
	svc := viewTagged{f0: "svc"}
	e := &env{key: "test.svc", v: svc}
	if got := e.operation("test.svc", "_tag"); got != "svc" {
		t.Errorf("env.operation on RecordView = %v, want svc", got)
	}
}

func TestRecordViewSameAndIdenticalDoNotPanicOnUncomparableFields(t *testing.T) {
	r1 := Rec("items", []Value{int32(1)}, "tag", "a")
	r2 := Rec("items", []Value{int32(1)}, "tag", "a")
	sharedSlice := []Value{int32(1), int32(2)}
	u1 := viewUncomparable{f0: sharedSlice, f1: "a"}
	u2 := viewUncomparable{f0: []Value{int32(1), int32(2)}, f1: "a"}
	fn := Fn(func(_ *Fiber, v Value) Value { return v })
	f1 := viewFunc{f0: fn, f1: 7}
	f2 := viewFunc{f0: fn, f1: 7}
	p1 := viewPoint{f0: 1.5, f1: 2.5}
	p2 := viewPoint{f0: 1.5, f1: 2.5}
	pDiff := viewPoint{f0: 1.5, f1: 3.0}
	ref1 := RefMake(true)
	ref2 := RefMake(true)
	refRec1 := viewUncomparable{f0: ref1, f1: "conn"}
	refRec1Copy := viewUncomparable{f0: ref1, f1: "conn"}
	refRec2 := viewUncomparable{f0: ref2, f1: "conn"}

	// `*Record` keeps pointer identity in both Same and identical.
	if !Same(r1, r1) || Same(r1, r2) {
		t.Errorf("Same on *Record broke pointer identity")
	}
	if !identical(r1, r1) || identical(r1, r2) {
		t.Errorf("identical on *Record broke pointer identity")
	}

	// `Same` never panics on value-type RecordView structs and returns false.
	for _, pair := range [][2]Value{
		{u1, u1},
		{u1, u2},
		{u1, r1},
		{r1, u1},
		{f1, f1},
		{f1, f2},
		{f1, r1},
		{p1, p2},
		{refRec1, refRec1Copy},
		{int32(1), u1},
		{u1, int32(1)},
	} {
		if Same(pair[0], pair[1]) {
			t.Errorf("Same(%T, %T) = true, want false", pair[0], pair[1])
		}
	}

	// `identical` and non-grouped `resolver.sameKey` compare non-`*Record`
	// RecordView values field-by-field via `identical` without raw Go `==`:
	// matching scalars, Refs, and shared slices succeed; distinct slices,
	// different field values, `*Record` vs struct, and `Fn` fields return false
	// without panicking.
	res := &resolver{}
	for _, pair := range [][2]Value{
		{p1, p2},
		{u1, u1},
		{refRec1, refRec1Copy},
	} {
		if !identical(pair[0], pair[1]) {
			t.Errorf("identical(%#v, %#v) = false, want true", pair[0], pair[1])
		}
		if !res.sameKey(pair[0], pair[1]) {
			t.Errorf("resolver.sameKey(%#v, %#v) = false, want true", pair[0], pair[1])
		}
	}
	for _, pair := range [][2]Value{
		{p1, pDiff},
		{u1, u2},
		{u1, r1},
		{r1, u1},
		{p1, r1},
		{r1, p1},
		{f1, f1},
		{f1, f2},
		{f1, r1},
		{refRec1, refRec2},
		{int32(1), u1},
		{u1, int32(1)},
	} {
		if identical(pair[0], pair[1]) {
			t.Errorf("identical(%T, %T) = true, want false", pair[0], pair[1])
		}
		if res.sameKey(pair[0], pair[1]) {
			t.Errorf("resolver.sameKey(%T, %T) = true, want false", pair[0], pair[1])
		}
	}
}

func TestPoolInvalidateAndReserveWithClosedRecordView(t *testing.T) {
	fb := rootFiber()
	fb.owner = newOwner()
	defer fb.owner.close(fb, nil)

	var nextID int32
	pool := PoolMake(fb, Rec(
		"acquire", Eff(func(_ *Fiber) Value {
			nextID++
			return viewUncomparable{f0: RefMake(true), f1: ShowInt(nextID)}
		}),
		"size", int32(1),
		"concurrency", Some(int32(2)),
		"targetUtilization", None,
	))
	p := poolOf(pool)

	// Lease a closed-record item, reserve it, verify reservation took effect,
	// then invalidate it so the next lease acquires a replacement item.
	lease1 := *fb
	lease1.owner = newOwner()
	got1 := PoolGet(&lease1, pool)
	if got1.(viewUncomparable).f1 != "1" {
		t.Fatalf("first leased item = %#v, want tag \"1\"", got1)
	}
	PoolReserve(&lease1, got1, pool)
	p.mu.Lock()
	found := p.find(got1)
	reserved, usage := 0, p.usage
	if found != nil {
		reserved = found.reservations
	}
	p.mu.Unlock()
	if found == nil || reserved != 1 || usage != 2 {
		lease1.owner.close(&lease1, nil)
		t.Fatalf("PoolReserve did not reserve closed-record item: found=%v reserved=%d usage=%d", found, reserved, usage)
	}

	PoolInvalidate(&lease1, got1, pool)
	lease1.owner.close(&lease1, nil)

	lease2 := *fb
	lease2.owner = newOwner()
	got2 := PoolGet(&lease2, pool)
	lease2.owner.close(&lease2, nil)
	if got2.(viewUncomparable).f1 != "2" {
		t.Fatalf("leased item after PoolInvalidate = %#v, want replacement tag \"2\"", got2)
	}
}

func TestDatabaseTransactionRejectsUncomparableRecordViewWithoutHashPanic(t *testing.T) {
	fn := Fn(func(_ *Fiber, v Value) Value { return v })
	for _, impl := range []Value{
		viewFunc{f0: fn, f1: 1},
		viewUncomparable{f0: []Value{int32(1)}, f1: "db"},
	} {
		fb := rootFiber()
		fb.env = fb.env.with(databaseKey, impl)
		var recovered any
		func() {
			defer func() { recovered = recover() }()
			DatabaseTransaction(fb, Eff(func(_ *Fiber) Value { return Unit }))
		}()
		defect, ok := recovered.(Defect)
		if !ok || defect.Message != "Error: this `Database` runs no transactions" {
			t.Fatalf("DatabaseTransaction(%T) panicked with %#v, want Defect{\"Error: this `Database` runs no transactions\"}", impl, recovered)
		}
	}
}
