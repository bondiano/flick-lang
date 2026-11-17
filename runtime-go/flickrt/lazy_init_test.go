package flickrt

import (
	"go/ast"
	"go/parser"
	"go/token"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
)

// TestRuntimePatterns holds the runtime to no regexp compiled at init and
// to no init function but trace.go's. It compiles every lazyRegexp, whose
// pattern must be a constant (a static literal, no code at init), and the
// constant sources of the checks that compile theirs on first use: init no
// longer proves them in every program.
func TestRuntimePatterns(t *testing.T) {
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	fset := token.NewFileSet()
	consts := map[string]ast.Expr{}
	var patterns []ast.Expr
	var checks []*ast.CallExpr // of patternCheck and isPattern
	for _, name := range files {
		if strings.HasSuffix(name, "_test.go") {
			continue
		}
		f, err := parser.ParseFile(fset, name, nil, 0)
		if err != nil {
			t.Fatal(err)
		}
		for _, d := range f.Decls {
			switch d := d.(type) {
			case *ast.FuncDecl:
				if d.Recv == nil && d.Name.Name == "init" && name != "trace.go" {
					t.Errorf("%s: an init function runs in every program", fset.Position(d.Pos()))
				}
			case *ast.GenDecl:
				for _, s := range d.Specs {
					vs, ok := s.(*ast.ValueSpec)
					if !ok {
						continue
					}
					for i, v := range vs.Values {
						if d.Tok == token.CONST {
							consts[vs.Names[i].Name] = v
						}
						ast.Inspect(v, func(n ast.Node) bool {
							if c, ok := n.(*ast.CallExpr); ok && isRegexpCompile(c.Fun) {
								t.Errorf("%s: a regexp compiled at init: use lazyRegexp", fset.Position(c.Pos()))
							}
							return true
						})
					}
				}
			}
		}
		ast.Inspect(f, func(n ast.Node) bool {
			if c, ok := n.(*ast.CallExpr); ok && (isIdent(c.Fun, "patternCheck") || isIdent(c.Fun, "isPattern")) {
				checks = append(checks, c)
			}
			lit, ok := n.(*ast.CompositeLit)
			if !ok {
				return true
			}
			if isIdent(lit.Type, "lazyRegexp") {
				patterns = append(patterns, lit)
			} else if a, ok := lit.Type.(*ast.ArrayType); ok && isIdent(a.Elt, "lazyRegexp") {
				patterns = append(patterns, lit.Elts...)
			}
			return true
		})
	}
	if len(patterns) == 0 {
		t.Fatal("no lazyRegexp literal found")
	}
	for _, p := range patterns {
		lit, ok := p.(*ast.CompositeLit)
		if !ok || len(lit.Elts) != 1 {
			t.Errorf("%s: write a lazyRegexp as {pattern: ...}", fset.Position(p.Pos()))
			continue
		}
		kv, ok := lit.Elts[0].(*ast.KeyValueExpr)
		if !ok || !isIdent(kv.Key, "pattern") {
			t.Errorf("%s: write a lazyRegexp as {pattern: ...}", fset.Position(p.Pos()))
			continue
		}
		source, ok := constString(kv.Value, consts)
		if !ok {
			t.Errorf("%s: the pattern is not a constant string", fset.Position(p.Pos()))
			continue
		}
		if _, err := regexp.Compile(source); err != nil {
			t.Errorf("%s: %v", fset.Position(p.Pos()), err)
		}
	}
	for _, c := range checks {
		source, ok := constString(c.Args[0], consts)
		if !ok {
			continue // made at run time, as CheckUUID's of a version
		}
		if isIdent(c.Fun, "patternCheck") {
			source = goPattern(utf16Units(source))
		}
		if _, err := regexp.Compile(source); err != nil {
			t.Errorf("%s: %v", fset.Position(c.Pos()), err)
		}
	}
}

func isIdent(e ast.Expr, name string) bool {
	id, ok := e.(*ast.Ident)
	return ok && id.Name == name
}

func isRegexpCompile(fun ast.Expr) bool {
	sel, ok := fun.(*ast.SelectorExpr)
	return ok && isIdent(sel.X, "regexp") && strings.HasSuffix(sel.Sel.Name, "Compile")
}

// constString is the value of a string constant made of literals, `+` and
// the package's constants.
func constString(e ast.Expr, consts map[string]ast.Expr) (string, bool) {
	switch e := e.(type) {
	case *ast.BasicLit:
		s, err := strconv.Unquote(e.Value)
		return s, e.Kind == token.STRING && err == nil
	case *ast.ParenExpr:
		return constString(e.X, consts)
	case *ast.BinaryExpr:
		x, okX := constString(e.X, consts)
		y, okY := constString(e.Y, consts)
		return x + y, e.Op == token.ADD && okX && okY
	case *ast.Ident:
		if c, ok := consts[e.Name]; ok {
			return constString(c, consts)
		}
	}
	return "", false
}

// Concurrent first users make a lazy value once and all get that value,
// under `go test -race`; a later use does not allocate.
func TestLazyFirstUseConcurrent(t *testing.T) {
	var made atomic.Int32
	var value onFirstUse[*int]
	pattern := lazyRegexp{pattern: `^a+$`}
	const users = 64
	ints := make([]*int, users)
	res := make([]*regexp.Regexp, users)
	start := make(chan struct{})
	var wg sync.WaitGroup
	for i := range users {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start
			ints[i] = value.get(func() *int { made.Add(1); return new(int) })
			res[i] = pattern.re()
		}()
	}
	close(start)
	wg.Wait()
	if made.Load() != 1 {
		t.Fatalf("made %d times", made.Load())
	}
	for i := range users {
		if ints[i] != ints[0] || res[i] != res[0] {
			t.Fatalf("user %d got another value", i)
		}
	}
	if !res[0].MatchString("aaa") || res[0].MatchString("ab") || pattern.re() != res[0] {
		t.Fatal("the pattern does not match as compiled")
	}
	if n := testing.AllocsPerRun(100, func() { pattern.re() }); n != 0 {
		t.Fatalf("a later use allocates %v times", n)
	}
}

// A lazy value whose make panics panics in its first user only; later
// users get the zero value and do not make it again.
func TestLazyFirstUsePanic(t *testing.T) {
	var value onFirstUse[*int]
	calls := 0
	boom := func() *int { calls++; panic("boom") }
	func() {
		defer func() {
			if recover() != "boom" {
				t.Fatal("the first use did not panic with make's value")
			}
		}()
		value.get(boom)
	}()
	if got := value.get(boom); got != nil || calls != 1 {
		t.Fatalf("a later use got %v after %d calls", got, calls)
	}
}
