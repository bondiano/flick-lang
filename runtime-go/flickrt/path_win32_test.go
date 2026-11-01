package flickrt

import (
	"bufio"
	"encoding/json"
	"os"
	"testing"
)

// TestPathWin32AsNode compares the win32 paths with Node's on the corpus
// `node tools/go-path/gen.mjs` writes; a `null` result is a thrown error.
func TestPathWin32AsNode(t *testing.T) {
	for _, name := range []string{"testdata/path_win32.txt", "testdata/path_win32_more.txt"} {
		f, err := os.Open(name)
		if err != nil {
			t.Fatal(err)
		}
		lines := bufio.NewScanner(f)
		for lines.Scan() {
			var line []json.RawMessage
			var op string
			var args []any
			if err := json.Unmarshal(lines.Bytes(), &line); err != nil {
				t.Fatal(err)
			}
			_ = json.Unmarshal(line[0], &op)
			_ = json.Unmarshal(line[1], &args)
			got, err := json.Marshal(win32Op(op, args))
			if err != nil {
				t.Fatal(err)
			}
			if string(got) != string(line[2]) && !sameJSON(got, line[2]) {
				t.Errorf("%s %s: got %s, Node gives %s", op, line[1], got, line[2])
			}
		}
		f.Close()
	}
}

func sameJSON(a, b []byte) bool {
	var x, y any
	_ = json.Unmarshal(a, &x)
	_ = json.Unmarshal(b, &y)
	ja, _ := json.Marshal(x)
	jb, _ := json.Marshal(y)
	return string(ja) == string(jb)
}

func win32Op(op string, args []any) any {
	str := func(i int) string { return args[i].(string) }
	strs := func() []string {
		out := make([]string, len(args))
		for i := range args {
			out[i] = str(i)
		}
		return out
	}
	parsed := func(v Value) any {
		r := record(v)
		out := map[string]string{}
		for _, k := range []string{"root", "dir", "base", "ext", "name"} {
			out[k] = r.Get(k).(string)
		}
		return out
	}
	failed := func(f func() Value) (out any) {
		defer func() {
			if recover() != nil {
				out = nil
			}
		}()
		if v, ok := f().(string); ok {
			return v
		}
		return nil
	}
	switch op {
	case "normalize":
		return win32Normalize(str(0))
	case "isAbsolute":
		return win32IsAbsolute(str(0))
	case "dirname":
		return win32Dirname(str(0))
	case "basename":
		if len(args) == 2 {
			return win32Basename(str(0), str(1))
		}
		return win32Basename(str(0), "")
	case "extname":
		return win32Extname(str(0))
	case "parse":
		return parsed(win32Parse(str(0)))
	case "format":
		if s, ok := args[0].(string); ok {
			return win32Format(win32Parse(s))
		}
		m := args[0].(map[string]any)
		return win32Format(Rec("root", m["root"], "dir", m["dir"], "base", m["base"], "ext", m["ext"], "name", m["name"]))
	case "join":
		return win32Join(strs())
	case "resolve":
		return win32Resolve(strs()...)
	case "relative":
		return win32Relative(str(0), str(1))
	case "toNamespacedPath":
		return win32ToNamespacedPath(str(0))
	case "fromFileUrl":
		return failed(func() Value { return win32FromFileUrl(str(0)) })
	case "toFileUrl":
		return failed(func() Value { return win32ToFileUrl(str(0)) })
	}
	panic("unknown operation " + op)
}
