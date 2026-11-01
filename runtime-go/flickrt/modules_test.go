package flickrt

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

// A program's go.mod requires only the modules of the optional packages it
// reaches, as listed in each package's modules.txt (flick-core/src/build/go.rs);
// the lists must name exactly what each package builds with, and together
// the runtime's own requirements, none of them asking for a newer Go.
func TestOptionalPackagesListTheirModules(t *testing.T) {
	root, err := filepath.Abs("..")
	if err != nil {
		t.Fatal(err)
	}
	goCmd := func(args ...string) string {
		cmd := exec.Command("go", args...)
		cmd.Dir = root
		out, err := cmd.Output()
		if err != nil {
			t.Fatalf("go %s: %v", strings.Join(args, " "), err)
		}
		return string(out)
	}
	var mod struct {
		Go      string
		Require []struct{ Path string }
	}
	if err := json.Unmarshal([]byte(goCmd("mod", "edit", "-json")), &mod); err != nil {
		t.Fatal(err)
	}
	var listed []string
	lists, _ := filepath.Glob(filepath.Join(root, "*", "modules.txt"))
	for _, path := range lists {
		text, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		modules := strings.Fields(string(text))
		pkg := "./" + filepath.Base(filepath.Dir(path))
		built := strings.Fields(goCmd("list", "-deps", "-f", "{{with .Module}}{{if not .Main}}{{.Path}}{{end}}{{end}}", pkg))
		slices.Sort(built)
		built = slices.Compact(built)
		if !slices.Equal(built, modules) {
			t.Errorf("%s: modules.txt lists %v, the package builds with %v", pkg, modules, built)
		}
		for _, module := range modules {
			if v := strings.TrimSpace(goCmd("list", "-m", "-f", "{{.GoVersion}}", module)); goVersionAbove(v, mod.Go) {
				t.Errorf("%s needs go %s, newer than the runtime's go %s", module, v, mod.Go)
			}
		}
		listed = append(listed, modules...)
	}
	var required []string
	for _, r := range mod.Require {
		required = append(required, r.Path)
	}
	slices.Sort(listed)
	listed = slices.Compact(listed)
	slices.Sort(required)
	if !slices.Equal(listed, required) {
		t.Errorf("the modules.txt files list %v, go.mod requires %v", listed, required)
	}
}

func goVersionAbove(v, limit string) bool {
	parts := func(s string) []int {
		var out []int
		for _, f := range strings.Split(s, ".") {
			n := 0
			for _, c := range f {
				if c < '0' || c > '9' {
					break
				}
				n = n*10 + int(c-'0')
			}
			out = append(out, n)
		}
		for len(out) < 3 {
			out = append(out, 0)
		}
		return out
	}
	return slices.Compare(parts(v), parts(limit)) > 0
}
