package flickrt

import (
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"math"
	"path/filepath"
	"strings"
	"testing"
)

// Nonconstant inputs and sinks so the Go compiler cannot constant-fold away
// typed Option construction, testing, or unwrapping.
var (
	optTestInt     int32   = 42
	optTestFloat   float64 = 3.25
	optTestBool    bool    = true
	optTestStr     string  = "hello"
	optTestRec     Value   = Rec("x", int32(7))
	optTestNilVal  Value   = nil
	optSinkInt     int32
	optSinkFloat   float64
	optSinkBool    bool
	optSinkStr     string
	optSinkVal     Value
	optSinkNested  Opt[Opt[int32]]
	optSinkRef     OptRef
	optSinkOptBool OptBool
)

// TestTypedOptionZeroAllocs proves 0 allocs/op for constructing, testing, and
// unwrapping concrete Options on both sentinel layouts (OptRef, OptBool) and
// presence-flag layouts (Opt[int32], Opt[float64], Opt[string], Opt[Value],
// nested Opt[Opt[int32]]), and measures erased Value conversion separately.
func TestTypedOptionZeroAllocs(t *testing.T) {
	// 1. Sentinel layout: OptRef (non-nil concrete payload, nil sentinel for None).
	if n := testing.AllocsPerRun(100, func() {
		o := SomeRef(optTestRec)
		if !o.IsSome() || o.IsNone() {
			t.Fatal("expected SomeRef to be Some")
		}
		optSinkRef = o
		optSinkVal = o.Unwrap()
		none := NoneRef
		if !none.IsNone() || none.IsSome() {
			t.Fatal("expected NoneRef to be None")
		}
	}); n != 0 {
		t.Errorf("sentinel OptRef construct/test/unwrap: %v allocs/op, want 0", n)
	}
	if optSinkVal != optTestRec {
		t.Fatalf("OptRef unwrapped %v, want %v", optSinkVal, optTestRec)
	}

	// 2. Sentinel layout: OptBool (1-byte sentinel 0=None, 1=Some(false), 2=Some(true)).
	if n := testing.AllocsPerRun(100, func() {
		o := SomeBool(optTestBool)
		if !o.IsSome() || o.IsNone() {
			t.Fatal("expected SomeBool to be Some")
		}
		optSinkOptBool = o
		optSinkBool = o.Unwrap()
		none := OptBoolNone
		if !none.IsNone() || none.IsSome() {
			t.Fatal("expected OptBoolNone to be None")
		}
	}); n != 0 {
		t.Errorf("sentinel OptBool construct/test/unwrap: %v allocs/op, want 0", n)
	}
	if !optSinkBool {
		t.Fatal("OptBool unwrapped false, want true")
	}

	// 3. Presence-flag layout: Opt[int32], Opt[float64], Opt[string], Opt[Value].
	if n := testing.AllocsPerRun(100, func() {
		o := SomeOpt(optTestInt)
		if !o.IsSome() || o.IsNone() {
			t.Fatal("expected Opt[int32] to be Some")
		}
		optSinkInt = o.Unwrap()
		none := NoneOpt[int32]()
		if !none.IsNone() || none.IsSome() {
			t.Fatal("expected NoneOpt[int32] to be None")
		}
	}); n != 0 {
		t.Errorf("presence-flag Opt[int32] construct/test/unwrap: %v allocs/op, want 0", n)
	}
	if optSinkInt != 42 {
		t.Fatalf("Opt[int32] unwrapped %d, want 42", optSinkInt)
	}

	if n := testing.AllocsPerRun(100, func() {
		of := SomeOpt(optTestFloat)
		if !of.IsSome() {
			t.Fatal("expected Opt[float64] to be Some")
		}
		optSinkFloat = of.Unwrap()
		os := SomeOpt(optTestStr)
		if !os.IsSome() {
			t.Fatal("expected Opt[string] to be Some")
		}
		optSinkStr = os.Unwrap()
		// Opt[Value] distinguishes Some(nil) from None.
		ov := SomeOpt(optTestNilVal)
		if !ov.IsSome() || ov.Unwrap() != nil {
			t.Fatal("expected Opt[Value] Some(nil) to be present with nil payload")
		}
		if NoneOpt[Value]().IsSome() {
			t.Fatal("expected Opt[Value] None to be absent")
		}
	}); n != 0 {
		t.Errorf("presence-flag Opt[float64]/Opt[string]/Opt[Value]: %v allocs/op, want 0", n)
	}

	// 4. Nested Option (Option (Option Int)) distinguishing None, Some None, Some (Some x).
	if n := testing.AllocsPerRun(100, func() {
		innerSome := SomeOpt(optTestInt)
		outerSome := SomeOpt(innerSome)
		someNone := SomeOpt(NoneOpt[int32]())
		outerNone := NoneOpt[Opt[int32]]()
		if outerNone.IsSome() || !someNone.IsSome() || someNone.Unwrap().IsSome() {
			t.Fatal("nested Option states conflated")
		}
		if !outerSome.IsSome() || !outerSome.Unwrap().IsSome() {
			t.Fatal("nested Some(Some x) not present")
		}
		optSinkNested = outerSome
		optSinkInt = outerSome.Unwrap().Unwrap()
	}); n != 0 {
		t.Errorf("nested Opt[Opt[int32]] construct/test/unwrap: %v allocs/op, want 0", n)
	}
	if optSinkInt != 42 {
		t.Fatalf("nested Opt[Opt[int32]] unwrapped %d, want 42", optSinkInt)
	}

	// 5. Conversion to erased flickrt.Value measured separately:
	//    - None conversions allocate 0 (reuse singleton None).
	//    - SomeRef conversion allocates 1 (*Data wrapper only).
	//    - BoxOpt(Some(None)) allocates 1 (outer *Data only; inner None is singleton).
	//    - BoxOptInt(Some(n)) allocates for the erased *Data (and boxed int32 if >255).
	if n := testing.AllocsPerRun(100, func() {
		optSinkVal = NoneRef.ToValue()
		optSinkVal = OptBoolNone.ToValue()
		optSinkVal = BoxOptInt(NoneOpt[int32]())
		optSinkVal = BoxOpt(NoneOpt[Opt[int32]](), BoxOptInt)
	}); n != 0 {
		t.Errorf("erased None conversion: %v allocs/op, want 0", n)
	}
	if n := testing.AllocsPerRun(100, func() {
		optSinkVal = SomeRef(optTestRec).ToValue()
	}); n != 1 {
		t.Errorf("erased SomeRef conversion: %v allocs/op, want 1", n)
	}
	if n := testing.AllocsPerRun(100, func() {
		optSinkVal = BoxOpt(SomeOpt(NoneOpt[int32]()), BoxOptInt)
	}); n != 1 {
		t.Errorf("erased Some(None) conversion: %v allocs/op, want 1", n)
	}
	if n := testing.AllocsPerRun(100, func() {
		optSinkVal = BoxOptInt(SomeOpt(optTestInt))
	}); n < 1 || n > 2 {
		t.Errorf("erased Some(int32) conversion: %v allocs/op, want 1..2", n)
	}
}

func TestTypedOptionNestingAndErasedHelpers(t *testing.T) {
	// Nested Option (Option (Option Int)) values:
	// None, Some None, Some (Some None), Some (Some (Some 1)) and Some (Some 1).
	box2 := func(o Opt[Opt[int32]]) Value { return BoxOpt(o, BoxOptInt) }
	unbox2 := func(v Value) Opt[Opt[int32]] { return UnboxOpt(v, AsOptInt) }

	vNone := BoxOpt(NoneOpt[Opt[Opt[int32]]](), box2)
	vSomeNone := BoxOpt(SomeOpt(NoneOpt[Opt[int32]]()), box2)
	vSomeSomeNone := BoxOpt(SomeOpt(SomeOpt(NoneOpt[int32]())), box2)
	vSomeSome1 := box2(SomeOpt(SomeOpt(int32(1))))

	if !IsNone(vNone) || IsSome(vNone) || TagOf(vNone) != "None" || ConstructorName(TagOf(vNone).(string)) != "None" || len(vNone.(*Data).Fields()) != 0 {
		t.Fatalf("erased None metadata broken: %#v", vNone)
	}
	if !IsSome(vSomeNone) || IsNone(vSomeNone) || !IsNone(UnwrapSome(vSomeNone)) {
		t.Fatalf("erased Some(None) broken: %#v", vSomeNone)
	}
	if !IsSome(vSomeSomeNone) || !IsSome(UnwrapSome(vSomeSomeNone)) || !IsNone(UnwrapSome(UnwrapSome(vSomeSomeNone))) {
		t.Fatalf("erased Some(Some(None)) broken: %#v", vSomeSomeNone)
	}
	if !IsSome(vSomeSome1) || !IsSome(UnwrapSome(vSomeSome1)) || UnwrapSome(UnwrapSome(vSomeSome1)) != int32(1) {
		t.Fatalf("erased Some(Some(1)) broken: %#v", vSomeSome1)
	}

	// Distinct erased representations for 1, Some 1, and Some (Some 1).
	raw1 := Value(int32(1))
	some1 := BoxOptInt(SomeOpt(int32(1)))
	someSome1 := box2(SomeOpt(SomeOpt(int32(1))))

	if IsSome(raw1) || IsNone(raw1) {
		t.Fatalf("raw 1 misidentified as Option")
	}
	if !IsSome(some1) || TagOf(some1) != "Some" || UnwrapSome(some1) != int32(1) {
		t.Fatalf("Some 1 erased representation wrong: %#v", some1)
	}
	if !IsSome(someSome1) || TagOf(someSome1) != "Some" || !IsSome(UnwrapSome(someSome1)) || UnwrapSome(UnwrapSome(someSome1)) != int32(1) {
		t.Fatalf("Some (Some 1) erased representation wrong: %#v", someSome1)
	}
	if Equals(raw1, some1) || Equals(some1, someSome1) || Equals(raw1, someSome1) {
		t.Fatalf("1, Some 1, and Some (Some 1) must not compare equal")
	}
	if Equals(vNone, vSomeNone) || Equals(vSomeNone, vSomeSomeNone) || Equals(vNone, vSomeSomeNone) {
		t.Fatalf("None, Some None, Some (Some None) must not compare equal")
	}

	// Unbox round-trips preserve nesting.
	rtSomeNone := unbox2(BoxOpt(SomeOpt(NoneOpt[int32]()), BoxOptInt))
	if !rtSomeNone.IsSome() || rtSomeNone.Unwrap().IsSome() {
		t.Fatalf("round-trip Some(None) failed: %#v", rtSomeNone)
	}
	rtSomeSome1 := unbox2(someSome1)
	if !rtSomeSome1.IsSome() || !rtSomeSome1.Unwrap().IsSome() || rtSomeSome1.Unwrap().Unwrap() != 1 {
		t.Fatalf("round-trip Some(Some(1)) failed: %#v", rtSomeSome1)
	}

	// Typed OptRef & OptBool round-trips and equality.
	if AsOptBool(SomeBool(false).ToValue()) != OptBoolFalse || AsOptBool(SomeBool(true).ToValue()) != OptBoolTrue || AsOptBool(OptBoolNone.ToValue()) != OptBoolNone {
		t.Fatal("OptBool round-trip failed")
	}
	r := Rec("a", int32(1))
	if got := AsOptRef(SomeRef(r).ToValue()); !got.IsSome() || !Equals(got.Unwrap(), r) {
		t.Fatalf("OptRef round-trip failed: %#v", got)
	}
	if got := AsOptRef(NoneRef.ToValue()); !got.IsNone() {
		t.Fatalf("OptRef None round-trip failed: %#v", got)
	}

	// Non-nil empty `more` slice on `&Data{Tag: "None"}` must behave identically
	// to `more == nil && n == 0` across IsNone, IsSome, and AsOpt* helpers.
	noneEmptyMore := &Data{Tag: "None", more: []Value{}}
	if !IsNone(noneEmptyMore) || IsSome(noneEmptyMore) {
		t.Fatalf("expected IsNone=true, IsSome=false for empty-more None")
	}
	if !AsOptInt(noneEmptyMore).IsNone() || !AsOptRef(noneEmptyMore).IsNone() || !AsOptBool(noneEmptyMore).IsNone() {
		t.Fatalf("AsOpt* must recognize empty-more None")
	}
	if IsNone(&Data{Tag: "None", more: []Value{int32(1)}}) {
		t.Fatal("None with non-empty more must not satisfy IsNone")
	}
	if IsNone(&Data{Tag: "None", n: 1, in: [2]Value{int32(1)}}) {
		t.Fatal("None with inline field must not satisfy IsNone")
	}

	// Float equality semantics on Opt[float64]: NaN == NaN, -0.0 == +0.0.
	if !EqOptFloat(SomeOpt(math.NaN()), SomeOpt(math.NaN())) {
		t.Fatal("EqOptFloat must treat Some(NaN) == Some(NaN)")
	}
	if !EqOptFloat(SomeOpt(math.Copysign(0, -1)), SomeOpt(0.0)) {
		t.Fatal("EqOptFloat must treat Some(-0.0) == Some(+0.0)")
	}
	if EqOptFloat(SomeOpt(0.0), NoneOpt[float64]()) {
		t.Fatal("EqOptFloat must distinguish Some(0.0) from None")
	}
	if !EqOptRef(SomeRef([]Value{int32(1)}), SomeRef([]Value{int32(1)})) {
		t.Fatal("EqOptRef must compare non-comparable slices structurally")
	}
	if EqOptRef(SomeRef([]Value{int32(1)}), NoneRef) {
		t.Fatal("EqOptRef must distinguish Some from None")
	}
}

// TestNoOptionTagReadersOutsideHelpers walks every Go source file in
// runtime-go and fails if any code outside the centralized helpers reads the
// "Some" or "None" tags of a Flick Option value directly.
func TestNoOptionTagReadersOutsideHelpers(t *testing.T) {
	root := filepath.Join("..")
	fset := token.NewFileSet()

	allowed := map[string]map[string]bool{
		// The centralized erased Option helpers and nullary cache.
		"flickrt/value.go": {
			"IsNone":     true,
			"IsSome":     true,
			"UnwrapSome": true,
			"Nullary":    true,
		},
		// JS native property-name table ("Some" -> "value") for generic hashing.
		"flickrt/data.go": {
			"nativeField": true,
		},
		// Unit tests explicitly asserting low-level *Data tags / TagOf on erased values.
		"flickrt/variant_test.go": {
			"TestAVariantHasTheFieldsItWasMadeWith":                       true,
			"TestAVariantOfFewFieldsIsOneAllocationKeptAndNoneUsedAtOnce": true,
		},
		"flickrt/option_typed_test.go": {
			"TestTypedOptionNestingAndErasedHelpers": true,
			"TestNoOptionTagReadersOutsideHelpers":   true,
		},
	}

	isOptionTagLit := func(expr ast.Expr) bool {
		lit, ok := expr.(*ast.BasicLit)
		return ok && lit.Kind == token.STRING && (lit.Value == `"Some"` || lit.Value == `"None"`)
	}
	isDirectTagExpr := func(expr ast.Expr) bool {
		switch e := expr.(type) {
		case *ast.SelectorExpr:
			return e.Sel.Name == "Tag"
		case *ast.CallExpr:
			switch fn := e.Fun.(type) {
			case *ast.Ident:
				return fn.Name == "TagOf" || fn.Name == "tagName" || fn.Name == "ConstructorName"
			case *ast.SelectorExpr:
				return fn.Sel.Name == "TagName" || fn.Sel.Name == "TagOf" || fn.Sel.Name == "ConstructorName"
			}
		}
		return false
	}

	var violations []string
	err := filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(path, ".go") {
			return nil
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		file, err := parser.ParseFile(fset, path, nil, 0)
		if err != nil {
			return err
		}
		allowedFuncs := allowed[rel]
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Body == nil {
				continue
			}
			if allowedFuncs[fn.Name.Name] {
				continue
			}
			// Track locals assigned directly from a tag-reading expression.
			tagVars := map[string]bool{}
			ast.Inspect(fn.Body, func(n ast.Node) bool {
				if assign, ok := n.(*ast.AssignStmt); ok {
					for i, rhs := range assign.Rhs {
						if i < len(assign.Lhs) && isDirectTagExpr(rhs) {
							if id, ok := assign.Lhs[i].(*ast.Ident); ok {
								tagVars[id.Name] = true
							}
						}
					}
				}
				return true
			})
			isTagExpr := func(expr ast.Expr) bool {
				if isDirectTagExpr(expr) {
					return true
				}
				id, ok := expr.(*ast.Ident)
				return ok && tagVars[id.Name]
			}
			ast.Inspect(fn.Body, func(n ast.Node) bool {
				switch node := n.(type) {
				case *ast.BinaryExpr:
					if (node.Op == token.EQL || node.Op == token.NEQ) &&
						((isTagExpr(node.X) && isOptionTagLit(node.Y)) ||
							(isTagExpr(node.Y) && isOptionTagLit(node.X))) {
						pos := fset.Position(node.Pos())
						violations = append(violations, rel+":"+filepath.Base(pos.String())+" compares tag directly against \"Some\"/\"None\" in "+fn.Name.Name)
					}
				case *ast.SwitchStmt:
					if node.Tag != nil && isTagExpr(node.Tag) {
						for _, stmt := range node.Body.List {
							cc, ok := stmt.(*ast.CaseClause)
							if !ok {
								continue
							}
							for _, expr := range cc.List {
								if isOptionTagLit(expr) {
									pos := fset.Position(expr.Pos())
									violations = append(violations, rel+":"+filepath.Base(pos.String())+" switches on tag against \"Some\"/\"None\" in "+fn.Name.Name)
								}
							}
						}
					}
				case *ast.CallExpr:
					name := ""
					switch f := node.Fun.(type) {
					case *ast.Ident:
						name = f.Name
					case *ast.SelectorExpr:
						name = f.Sel.Name
					}
					if (name == "IsTag" || name == "IsData") && len(node.Args) >= 2 && isOptionTagLit(node.Args[1]) {
						pos := fset.Position(node.Pos())
						violations = append(violations, rel+":"+filepath.Base(pos.String())+" calls "+name+" with \"Some\"/\"None\" in "+fn.Name.Name)
					}
				}
				return true
			})
		}
		return nil
	})
	if err != nil {
		t.Fatalf("walking runtime-go: %v", err)
	}
	if len(violations) > 0 {
		t.Fatalf("found Option tag readers outside helpers:\n%s", strings.Join(violations, "\n"))
	}
}
