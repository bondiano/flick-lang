package flickrt

import (
	"runtime"
	"testing"
	"time"
)

// procShell is the command line in the system's shell.
func procShell(script string) Value {
	if runtime.GOOS == "windows" {
		return ChildProcessMake("cmd", []Value{"/d", "/c", script})
	}
	return ChildProcessMake("sh", []Value{"-c", script})
}

func procSlow() Value {
	if runtime.GOOS == "windows" {
		return procShell("ping -n 30 127.0.0.1 > nul")
	}
	return procShell("sleep 30")
}

func TestChildProcessExitCodeOutputAndPipeline(t *testing.T) {
	fb := rootFiber()
	if got := ChildProcessExitCode(fb, procShell("exit 3")); got != int32(3) {
		t.Fatalf("exit code %v", got)
	}
	lines := ChildProcessLines(fb, ChildProcessPipeTo(ChildProcessMake("sort", []Value{}), procShell("echo b&& echo a")))
	if got := Format(lines); got != `["a","b"]` {
		t.Fatalf("pipeline gave %s", got)
	}
	text := ChildProcessString(fb, ChildProcessSetStdin("x1\n", procShell(map[bool]string{true: "findstr x", false: "cat"}[runtime.GOOS == "windows"])))
	if got := text.(string); got != "x1\n" && got != "x1\r\n" {
		t.Fatalf("stdin echoed %q", got)
	}
}

// Killing a process ends its tree, and a closing scope stops what still runs.
func TestChildProcessKillAndScope(t *testing.T) {
	fb := rootFiber()
	var p *process
	start := time.Now()
	owned(fb, func(fb *Fiber, _ *Owner) Value {
		p = ChildProcessSpawn(fb, procSlow()).(*process)
		if !p.running() {
			t.Fatal("not running after spawn")
		}
		ChildProcessKill(fb, p)
		if p.running() {
			t.Fatal("still running after kill")
		}
		p = ChildProcessSpawn(fb, procSlow()).(*process)
		return Unit
	})
	if p.running() {
		t.Fatal("the scope left the process running")
	}
	if time.Since(start) > 10*time.Second {
		t.Fatalf("stopping took %v", time.Since(start))
	}
}

func TestChildProcessMissingProgramIsNotFound(t *testing.T) {
	_, cause := catch(func() Value { return ChildProcessExitCode(rootFiber(), ChildProcessMake("flick-no-such-program", []Value{})) })
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("no failure: %v", cause)
	}
	reason := cause.err.(*Data).Fields[0].(*Data).Fields[0].(*Record)
	if tag := reason.Get("tag").(*Data).Tag; tag != Tag("PlatformError.SystemErrorTag.NotFound") {
		t.Fatalf("tag %s", tag)
	}
}
