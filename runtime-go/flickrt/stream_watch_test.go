package flickrt

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

// watchTest watches the path through `FileSystem.node` on a goroutine of
// its own, sending each event as "Kind path"; `stop` interrupts it and
// gives how it ended, once its scope closed.
func watchTest(t *testing.T, path string, recursive bool) (<-chan string, func() *Cause) {
	t.Helper()
	events := make(chan string, 1024)
	ended := make(chan *Cause, 1)
	opened := make(chan struct{})
	fb := rootFiber()
	go func() {
		_, cause := catch(func() Value {
			return Provide(fb, FileSystemNode("FileSystem.FileSystem"), Eff(func(fb *Fiber) Value {
				return owned(fb, func(fb *Fiber, _ *Owner) Value {
					next := StreamWatch(Rec("recursive", recursive), path).(*Stream).open(fb)
					close(opened)
					for {
						chunk, ok := next(fb)
						if !ok {
							return Unit
						}
						for _, e := range chunk {
							d := e.(*Data)
							events <- ConstructorName(d.Tag) + " " + d.Fields()[0].(string)
						}
					}
				})
			}))
		})
		ended <- cause
	}()
	select {
	case <-opened:
	case cause := <-ended:
		ended <- cause
	}
	return events, func() *Cause {
		fb.state.interrupt(0)
		select {
		case cause := <-ended:
			return cause
		case <-time.After(time.Second):
			t.Fatal("the watch ignored its interruption for 1 s")
			return nil
		}
	}
}

// watchSees waits up to 5 s for each of the events, in any order, and
// gives every event seen meanwhile.
func watchSees(t *testing.T, events <-chan string, want ...string) []string {
	t.Helper()
	missing := map[string]bool{}
	for _, w := range want {
		missing[w] = true
	}
	var seen []string
	deadline := time.After(5 * time.Second)
	for len(missing) > 0 {
		select {
		case e := <-events:
			seen = append(seen, e)
			delete(missing, e)
		case <-deadline:
			t.Fatalf("within 5 s, no %v among %v", missing, seen)
		}
	}
	return seen
}

func watchWrite(t *testing.T, path, text string, flag int) {
	t.Helper()
	f, err := os.OpenFile(path, flag|os.O_WRONLY, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString(text)
	f.Close()
}

func TestWatchReportsCreationsUpdatesRemovalsAndRenames(t *testing.T) {
	dir := t.TempDir()
	events, stop := watchTest(t, dir, false)
	defer stop()
	a, b := filepath.Join(dir, "a.txt"), filepath.Join(dir, "b.txt")
	watchWrite(t, a, "one", os.O_CREATE)
	watchSees(t, events, "Create a.txt")
	watchWrite(t, a, " two", os.O_APPEND)
	watchSees(t, events, "Update a.txt")
	if err := os.Rename(a, b); err != nil {
		t.Fatal(err)
	}
	watchSees(t, events, "Remove a.txt", "Create b.txt")
	if err := os.Remove(b); err != nil {
		t.Fatal(err)
	}
	watchSees(t, events, "Remove b.txt")
}

func TestWatchRecursiveReportsNestedPathsAndNewDirectories(t *testing.T) {
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "d", "e"), 0o755); err != nil {
		t.Fatal(err)
	}
	events, stop := watchTest(t, dir, true)
	defer stop()
	watchWrite(t, filepath.Join(dir, "d", "e", "f.txt"), "f", os.O_CREATE)
	watchSees(t, events, "Create "+filepath.Join("d", "e", "f.txt"))
	if err := os.Mkdir(filepath.Join(dir, "new"), 0o755); err != nil {
		t.Fatal(err)
	}
	watchSees(t, events, "Create new")
	watchWrite(t, filepath.Join(dir, "new", "c.txt"), "c", os.O_CREATE)
	watchSees(t, events, "Create "+filepath.Join("new", "c.txt"))
	if err := os.RemoveAll(filepath.Join(dir, "new")); err != nil {
		t.Fatal(err)
	}
	watchSees(t, events, "Remove new")
}

func TestWatchNonRecursiveLeavesSubdirectoriesOut(t *testing.T) {
	dir := t.TempDir()
	if err := os.Mkdir(filepath.Join(dir, "sub"), 0o755); err != nil {
		t.Fatal(err)
	}
	events, stop := watchTest(t, dir, false)
	defer stop()
	watchWrite(t, filepath.Join(dir, "sub", "x.txt"), "x", os.O_CREATE)
	time.Sleep(100 * time.Millisecond)
	watchWrite(t, filepath.Join(dir, "y.txt"), "y", os.O_CREATE)
	for _, e := range watchSees(t, events, "Create y.txt") {
		if strings.Contains(e, "x.txt") {
			t.Errorf("a non-recursive watch reported %q", e)
		}
	}
}

func TestWatchFileReportsItsChangesByName(t *testing.T) {
	dir := t.TempDir()
	f := filepath.Join(dir, "f.txt")
	watchWrite(t, f, "f", os.O_CREATE)
	events, stop := watchTest(t, f, false)
	defer stop()
	watchWrite(t, filepath.Join(dir, "g.txt"), "g", os.O_CREATE)
	watchWrite(t, f, "more", os.O_APPEND)
	for _, e := range watchSees(t, events, "Update f.txt") {
		if strings.Contains(e, "g.txt") {
			t.Errorf("a watch of a file reported %q", e)
		}
	}
}

func TestWatchFailsOnAMissingPathOrAnotherFileSystem(t *testing.T) {
	missing := filepath.Join(t.TempDir(), "none")
	var fields *Record
	Provide(rootFiber(), FileSystemNode("FileSystem.FileSystem"), Eff(func(fb *Fiber) Value {
		fields = fsTestFailure(t, func() Value {
			StreamWatch(Rec("recursive", false), missing).(*Stream).open(fb)(fb)
			return nil
		}, "NotFound")
		return Unit
	}))
	if fields.Get("method") != "stat" {
		t.Errorf("a missing path fails in %v", fields.Get("method"))
	}
	fields = fsTestFailure(t, func() Value {
		fb := rootFiber()
		StreamWatch(Rec("recursive", false), t.TempDir()).(*Stream).open(fb)(fb)
		return nil
	}, "NotFound")
	if fields.Get("method") != "watch" {
		t.Errorf("another file system fails in %v", fields.Get("method"))
	}
}

func TestWatchEndsItsGoroutinesWhenInterrupted(t *testing.T) {
	before := runtime.NumGoroutine()
	_, stop := watchTest(t, t.TempDir(), true)
	if cause := stop(); cause == nil || !cause.has(causeInterrupt) {
		t.Fatalf("the watch ended with %v", cause)
	}
	deadline := time.Now().Add(5 * time.Second)
	for runtime.NumGoroutine() > before && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	if n := runtime.NumGoroutine(); n > before {
		t.Errorf("%d goroutines, %d before the watch", n, before)
	}
}
