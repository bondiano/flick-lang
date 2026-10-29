package flickrt

import (
	"os"
	"path/filepath"
	"testing"
)

func TestAFileClosesWithItsScope(t *testing.T) {
	path := filepath.Join(t.TempDir(), "f.txt")
	if err := os.WriteFile(path, []byte("text"), 0o600); err != nil {
		t.Fatal(err)
	}
	var f Value
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		f = FileOpen(fb, path)
		if got := FileReadHandle(fb, f); got != "text" {
			t.Errorf("read %v", got)
		}
		return Unit
	})
	if _, err := f.(*os.File).Read(make([]byte, 1)); err == nil {
		t.Fatal("the file is still open after its scope closed")
	}
	if got := nodeMessage(os.ErrNotExist, "open", "/x"); got == "" {
		t.Fatal("a message")
	}
	_, err := os.Open("/nonexistent/x")
	if got := nodeMessage(err, "open", "/nonexistent/x"); got != "ENOENT: no such file or directory, open '/nonexistent/x'" {
		t.Fatalf("got %q", got)
	}
}
