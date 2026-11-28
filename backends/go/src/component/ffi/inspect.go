package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"go/ast"
	"go/constant"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"io"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

type listPkg struct {
	Dir        string            `json:"Dir"`
	ImportPath string            `json:"ImportPath"`
	Name       string            `json:"Name"`
	Standard   bool              `json:"Standard"`
	Export     string            `json:"Export"`
	GoFiles    []string          `json:"GoFiles"`
	CgoFiles   []string          `json:"CgoFiles"`
	ImportMap  map[string]string `json:"ImportMap"`
	Module     *listModule       `json:"Module"`
	Error      *listError        `json:"Error"`
}

type listModule struct {
	Path    string `json:"Path"`
	Version string `json:"Version"`
	Main    bool   `json:"Main"`
}

type listError struct {
	Err string `json:"Err"`
}

type surface struct {
	Package      string        `json:"package"`
	Version      *string       `json:"version,omitempty"`
	Declarations string        `json:"declarations"`
	Exports      []exportEntry `json:"exports"`
}

type exportEntry struct {
	Name        string     `json:"name"`
	Kind        string     `json:"kind"`
	Receiver    *receiver  `json:"receiver,omitempty"`
	Signature   *signature `json:"signature,omitempty"`
	Type        *typeShape `json:"type,omitempty"`
	ConstKind   string     `json:"const_kind,omitempty"`
	Int32OK     *bool      `json:"int32_ok,omitempty"`
	Fingerprint string     `json:"fingerprint"`
}

type receiver struct {
	Package     string `json:"package"`
	Name        string `json:"name"`
	Ptr         bool   `json:"ptr"`
	IsInterface bool   `json:"is_interface"`
	Generic     bool   `json:"generic"`
}

type signature struct {
	TypeParams []string    `json:"type_params,omitempty"`
	Params     []param     `json:"params"`
	Results    []typeShape `json:"results"`
	Variadic   bool        `json:"variadic"`
}

type param struct {
	Name string    `json:"name"`
	Type typeShape `json:"type"`
}

type typeShape struct {
	Kind        string      `json:"kind"`
	Basic       string      `json:"basic,omitempty"`
	Elem        *typeShape  `json:"elem,omitempty"`
	Package     string      `json:"package,omitempty"`
	Name        string      `json:"name,omitempty"`
	Exported    bool        `json:"exported,omitempty"`
	Underlying  string      `json:"underlying,omitempty"`
	TypeArgs    []typeShape `json:"type_args,omitempty"`
	IsError     bool        `json:"is_error,omitempty"`
	IsContext   bool        `json:"is_context,omitempty"`
	IsInterface bool        `json:"is_interface,omitempty"`
	Text        string      `json:"text"`
}

func main() {
	outPath := flag.String("out", "", "output JSON path")
	modDir := flag.String("moddir", "", "optional Go module directory")
	targetDir := flag.String("targetdir", "", "optional synthesized Go module directory for third-party deps")
	flag.Parse()

	if *outPath == "" || flag.NArg() != 1 {
		fmt.Fprintln(os.Stderr, "usage: inspect --out <file> [--moddir <dir>] [--targetdir <dir>] <import-path>")
		os.Exit(2)
	}
	importPath := flag.Arg(0)

	s, err := inspectPackage(importPath, *modDir, *targetDir)
	if err != nil {
		fmt.Fprintln(os.Stderr, err.Error())
		os.Exit(1)
	}
	data, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		fmt.Fprintf(os.Stderr, "encode JSON: %v\n", err)
		os.Exit(1)
	}
	data = append(data, '\n')
	if err := os.WriteFile(*outPath, data, 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "write %s: %v\n", *outPath, err)
		os.Exit(1)
	}
}

func isStdlibPath(importPath string) bool {
	first := importPath
	if idx := strings.IndexByte(importPath, '/'); idx >= 0 {
		first = importPath[:idx]
	}
	return !strings.ContainsRune(first, '.')
}

func inspectPackage(importPath, modDir, targetDir string) (*surface, error) {
	listDir := "."
	modFlag := "-mod=readonly"
	if modDir != "" {
		listDir = modDir
		modFlag = "-mod=readonly"
	} else if !isStdlibPath(importPath) && targetDir != "" {
		listDir = targetDir
		modFlag = "-mod=mod"
	}

	cmd := exec.Command("go", "list", modFlag, "-e", "-export", "-json", "-deps", "--", importPath)
	cmd.Dir = listDir
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		msg := strings.TrimSpace(stderr.String())
		if msg == "" {
			msg = err.Error()
		}
		return nil, fmt.Errorf("cannot load Go package %q: %s", importPath, msg)
	}

	dec := json.NewDecoder(&stdout)
	exportsByPath := make(map[string]string)
	var target *listPkg
	for {
		var p listPkg
		err := dec.Decode(&p)
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("decode `go list` output for %q: %w", importPath, err)
		}
		if p.Export != "" {
			exportsByPath[p.ImportPath] = p.Export
		}
		if p.ImportPath == importPath {
			cp := p
			target = &cp
		}
	}
	if target == nil {
		msg := strings.TrimSpace(stderr.String())
		if msg != "" {
			return nil, fmt.Errorf("Go package %q not found: %s", importPath, msg)
		}
		return nil, fmt.Errorf("Go package %q not found", importPath)
	}
	if target.Error != nil {
		return nil, fmt.Errorf("cannot load Go package %q: %s", importPath, target.Error.Err)
	}

	pkg, err := loadTypesPackage(target, exportsByPath)
	if err != nil {
		return nil, fmt.Errorf("type-check Go package %q: %w", importPath, err)
	}

	var version *string
	if !target.Standard && target.Module != nil && target.Module.Version != "" {
		v := target.Module.Version
		version = &v
	}

	exports := collectExports(pkg)
	return &surface{
		Package:      importPath,
		Version:      version,
		Declarations: fmt.Sprintf("go/types (%s)", pkg.Name()),
		Exports:      exports,
	}, nil
}

func loadTypesPackage(target *listPkg, exportsByPath map[string]string) (*types.Package, error) {
	fset := token.NewFileSet()
	lookup := func(path string) (io.ReadCloser, error) {
		if target.ImportMap != nil {
			if mapped, ok := target.ImportMap[path]; ok {
				path = mapped
			}
		}
		exp, ok := exportsByPath[path]
		if !ok || exp == "" {
			return nil, fmt.Errorf("no export data for %q", path)
		}
		return os.Open(exp)
	}
	imp := importer.ForCompiler(fset, "gc", lookup)

	// Prefer compiled gc export data produced by `go list -export`: it handles
	// build tags, cgo, assembly, and stdlib internals identically to `go build`.
	if target.Export != "" {
		if pkg, err := imp.Import(target.ImportPath); err == nil {
			return pkg, nil
		}
	}

	if len(target.GoFiles) == 0 {
		return nil, fmt.Errorf("package %q has no Go source files or export data", target.ImportPath)
	}
	files := make([]*ast.File, 0, len(target.GoFiles))
	for _, name := range target.GoFiles {
		full := filepath.Join(target.Dir, name)
		f, err := parser.ParseFile(fset, full, nil, 0)
		if err != nil {
			return nil, err
		}
		files = append(files, f)
	}
	cfg := types.Config{
		Importer: imp,
	}
	return cfg.Check(target.ImportPath, fset, files, nil)
}

func collectExports(pkg *types.Package) []exportEntry {
	scope := pkg.Scope()
	names := scope.Names()
	sort.Strings(names)

	var exports []exportEntry
	for _, name := range names {
		if !token.IsExported(name) {
			continue
		}
		obj := scope.Lookup(name)
		switch o := obj.(type) {
		case *types.Func:
			sig := encodeSignature(o.Type().(*types.Signature))
			entry := exportEntry{
				Name:      name,
				Kind:      "func",
				Signature: &sig,
			}
			entry.Fingerprint = computeFingerprint(&entry)
			exports = append(exports, entry)

		case *types.Const:
			ty := encodeType(o.Type())
			ck, i32ok := encodeConstInfo(o.Val())
			entry := exportEntry{
				Name:      name,
				Kind:      "const",
				Type:      &ty,
				ConstKind: ck,
				Int32OK:   i32ok,
			}
			entry.Fingerprint = computeFingerprint(&entry)
			exports = append(exports, entry)

		case *types.Var:
			ty := encodeType(o.Type())
			entry := exportEntry{
				Name: name,
				Kind: "var",
				Type: &ty,
			}
			entry.Fingerprint = computeFingerprint(&entry)
			exports = append(exports, entry)

		case *types.TypeName:
			if o.IsAlias() {
				continue
			}
			named, ok := o.Type().(*types.Named)
			if !ok {
				continue
			}
			_, isIface := named.Underlying().(*types.Interface)
			isGeneric := named.TypeParams().Len() > 0

			var mset *types.MethodSet
			if isIface {
				mset = types.NewMethodSet(named)
			} else {
				mset = types.NewMethodSet(types.NewPointer(named))
			}

			var methods []exportEntry
			for i := 0; i < mset.Len(); i++ {
				sel := mset.At(i)
				fn, ok := sel.Obj().(*types.Func)
				if !ok || !token.IsExported(fn.Name()) {
					continue
				}
				sigObj := sel.Type().(*types.Signature)
				sig := encodeSignature(sigObj)
				recvPtr := false
				if !isIface {
					if origSig, ok := fn.Type().(*types.Signature); ok && origSig.Recv() != nil {
						_, recvPtr = origSig.Recv().Type().(*types.Pointer)
					}
				}
				recv := &receiver{
					Package:     pkg.Path(),
					Name:        name,
					Ptr:         recvPtr,
					IsInterface: isIface,
					Generic:     isGeneric,
				}
				entry := exportEntry{
					Name:      name + "." + fn.Name(),
					Kind:      "method",
					Receiver:  recv,
					Signature: &sig,
				}
				entry.Fingerprint = computeFingerprint(&entry)
				methods = append(methods, entry)
			}
			sort.Slice(methods, func(i, j int) bool {
				return methods[i].Name < methods[j].Name
			})
			exports = append(exports, methods...)
		}
	}

	sort.Slice(exports, func(i, j int) bool {
		return exports[i].Name < exports[j].Name
	})
	return exports
}

func encodeConstInfo(v constant.Value) (string, *bool) {
	if v == nil {
		return "", nil
	}
	switch v.Kind() {
	case constant.Bool:
		return "bool", nil
	case constant.String:
		return "string", nil
	case constant.Int:
		ok := false
		if n, exact := constant.Int64Val(v); exact && n >= math.MinInt32 && n <= math.MaxInt32 {
			ok = true
		}
		return "int", &ok
	case constant.Float:
		return "float", nil
	case constant.Complex:
		return "complex", nil
	default:
		return "unknown", nil
	}
}

func encodeSignature(sig *types.Signature) signature {
	var tparams []string
	if tp := sig.TypeParams(); tp != nil {
		for i := 0; i < tp.Len(); i++ {
			tparams = append(tparams, tp.At(i).Obj().Name())
		}
	}
	ps := sig.Params()
	params := make([]param, ps.Len())
	for i := 0; i < ps.Len(); i++ {
		v := ps.At(i)
		params[i] = param{
			Name: v.Name(),
			Type: encodeType(v.Type()),
		}
	}
	rs := sig.Results()
	results := make([]typeShape, rs.Len())
	for i := 0; i < rs.Len(); i++ {
		results[i] = encodeType(rs.At(i).Type())
	}
	return signature{
		TypeParams: tparams,
		Params:     params,
		Results:    results,
		Variadic:   sig.Variadic(),
	}
}

func encodeType(t types.Type) typeShape {
	text := types.TypeString(t, func(p *types.Package) string {
		if p == nil {
			return ""
		}
		return p.Path()
	})
	// Unalias Go 1.22+ explicit type aliases so `any` -> `interface{}` and
	// `byte` -> `uint8` / `rune` -> `int32` are classified consistently, while
	// keeping `byte` and `rune` distinguishable by name when helpful.
	if alias, ok := t.(*types.Alias); ok {
		if alias.Obj().Pkg() == nil {
			switch alias.Obj().Name() {
			case "any":
				return typeShape{Kind: "empty_interface", Text: "any"}
			case "byte":
				return typeShape{Kind: "basic", Basic: "byte", Text: "byte"}
			case "rune":
				return typeShape{Kind: "basic", Basic: "rune", Text: "rune"}
			}
		}
		t = types.Unalias(t)
	}

	switch u := t.(type) {
	case *types.Basic:
		if u.Name() == "unsafe.Pointer" {
			return typeShape{Kind: "unsafe_pointer", Text: "unsafe.Pointer"}
		}
		return typeShape{
			Kind:  "basic",
			Basic: basicKindName(u),
			Text:  u.Name(),
		}

	case *types.Slice:
		elem := encodeType(u.Elem())
		return typeShape{
			Kind: "slice",
			Elem: &elem,
			Text: text,
		}

	case *types.Pointer:
		elem := encodeType(u.Elem())
		return typeShape{
			Kind: "pointer",
			Elem: &elem,
			Text: text,
		}

	case *types.Named:
		obj := u.Obj()
		pkgPath := ""
		if obj.Pkg() != nil {
			pkgPath = obj.Pkg().Path()
		}
		isError := pkgPath == "" && obj.Name() == "error"
		isContext := pkgPath == "context" && obj.Name() == "Context"
		_, isIface := u.Underlying().(*types.Interface)
		var targs []typeShape
		if ta := u.TypeArgs(); ta != nil {
			for i := 0; i < ta.Len(); i++ {
				targs = append(targs, encodeType(ta.At(i)))
			}
		}
		if len(targs) == 0 && u.TypeParams().Len() > 0 {
			for i := 0; i < u.TypeParams().Len(); i++ {
				targs = append(targs, encodeType(u.TypeParams().At(i)))
			}
		}
		return typeShape{
			Kind:        "named",
			Package:     pkgPath,
			Name:        obj.Name(),
			Exported:    obj.Exported() || isError,
			Underlying:  underlyingKind(u.Underlying()),
			TypeArgs:    targs,
			IsError:     isError,
			IsContext:   isContext,
			IsInterface: isIface,
			Text:        text,
		}

	case *types.Interface:
		if u.Empty() {
			return typeShape{Kind: "empty_interface", Text: text}
		}
		return typeShape{Kind: "interface_anon", Text: text}

	case *types.Chan:
		return typeShape{Kind: "chan", Text: text}

	case *types.Map:
		return typeShape{Kind: "map", Text: text}

	case *types.Array:
		return typeShape{Kind: "array", Text: text}

	case *types.Signature:
		return typeShape{Kind: "func", Text: text}

	case *types.Struct:
		return typeShape{Kind: "struct_anon", Text: text}

	case *types.TypeParam:
		return typeShape{Kind: "type_param", Name: u.Obj().Name(), Text: text}

	default:
		return typeShape{Kind: "other", Text: text}
	}
}

func basicKindName(b *types.Basic) string {
	switch b.Kind() {
	case types.Bool:
		return "bool"
	case types.Int:
		return "int"
	case types.Int8:
		return "int8"
	case types.Int16:
		return "int16"
	case types.Int32:
		if b.Name() == "rune" {
			return "rune"
		}
		return "int32"
	case types.Int64:
		return "int64"
	case types.Uint:
		return "uint"
	case types.Uint8:
		if b.Name() == "byte" {
			return "byte"
		}
		return "uint8"
	case types.Uint16:
		return "uint16"
	case types.Uint32:
		return "uint32"
	case types.Uint64:
		return "uint64"
	case types.Uintptr:
		return "uintptr"
	case types.Float32:
		return "float32"
	case types.Float64:
		return "float64"
	case types.Complex64:
		return "complex64"
	case types.Complex128:
		return "complex128"
	case types.String:
		return "string"
	case types.UntypedBool:
		return "untyped_bool"
	case types.UntypedInt:
		return "untyped_int"
	case types.UntypedRune:
		return "untyped_rune"
	case types.UntypedFloat:
		return "untyped_float"
	case types.UntypedComplex:
		return "untyped_complex"
	case types.UntypedString:
		return "untyped_string"
	default:
		return b.Name()
	}
}

func underlyingKind(t types.Type) string {
	switch t.(type) {
	case *types.Struct:
		return "struct"
	case *types.Interface:
		return "interface"
	case *types.Basic:
		return "basic"
	case *types.Slice:
		return "slice"
	case *types.Map:
		return "map"
	case *types.Chan:
		return "chan"
	case *types.Signature:
		return "func"
	case *types.Array:
		return "array"
	case *types.Pointer:
		return "pointer"
	default:
		return "other"
	}
}

// computeFingerprint hashes the canonical structural shape of an export,
// ignoring parameter names so formatting or renaming parameters does not
// cause false drift while any type, arity, receiver, or variadic change does.
func computeFingerprint(e *exportEntry) string {
	var b strings.Builder
	b.WriteString(e.Kind)
	b.WriteByte('|')
	b.WriteString(e.Name)
	if e.Receiver != nil {
		fmt.Fprintf(&b, "|recv:%s.%s:ptr=%v:iface=%v:gen=%v",
			e.Receiver.Package, e.Receiver.Name, e.Receiver.Ptr, e.Receiver.IsInterface, e.Receiver.Generic)
	}
	if e.Signature != nil {
		b.WriteString("|tp:[")
		b.WriteString(strings.Join(e.Signature.TypeParams, ","))
		b.WriteString("]|params:[")
		for i, p := range e.Signature.Params {
			if i > 0 {
				b.WriteByte(',')
			}
			writeCanonicalType(&b, &p.Type)
		}
		fmt.Fprintf(&b, "]|var=%v|results:[", e.Signature.Variadic)
		for i, r := range e.Signature.Results {
			if i > 0 {
				b.WriteByte(',')
			}
			writeCanonicalType(&b, &r)
		}
		b.WriteByte(']')
	}
	if e.Type != nil {
		b.WriteString("|type:")
		writeCanonicalType(&b, e.Type)
	}
	if e.ConstKind != "" {
		b.WriteString("|const:")
		b.WriteString(e.ConstKind)
		if e.Int32OK != nil {
			fmt.Fprintf(&b, ":i32=%v", *e.Int32OK)
		}
	}
	sum := sha256.Sum256([]byte(b.String()))
	return "sha256:" + hex.EncodeToString(sum[:])
}

func writeCanonicalType(b *strings.Builder, t *typeShape) {
	b.WriteString(t.Kind)
	switch t.Kind {
	case "basic":
		b.WriteByte(':')
		b.WriteString(t.Basic)
	case "slice", "pointer":
		b.WriteByte('(')
		if t.Elem != nil {
			writeCanonicalType(b, t.Elem)
		}
		b.WriteByte(')')
	case "named":
		fmt.Fprintf(b, ":%s.%s:exp=%v:u=%s:err=%v:ctx=%v:iface=%v",
			t.Package, t.Name, t.Exported, t.Underlying, t.IsError, t.IsContext, t.IsInterface)
		if len(t.TypeArgs) > 0 {
			b.WriteByte('[')
			for i := range t.TypeArgs {
				if i > 0 {
					b.WriteByte(',')
				}
				writeCanonicalType(b, &t.TypeArgs[i])
			}
			b.WriteByte(']')
		}
	case "type_param":
		b.WriteByte(':')
		b.WriteString(t.Name)
	default:
		b.WriteByte(':')
		b.WriteString(t.Text)
	}
}
