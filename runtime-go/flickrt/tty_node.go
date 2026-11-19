package flickrt

import (
	"os"
	"sync"
)

// The terminal of the process for prompts, as Effect's NodeTerminal: stdin
// in raw mode while a prompt may read it, its key presses decoded, frames
// written to stdout. Escape, Ctrl+C and Ctrl+D are read and then end the
// input, which later reads fail on with `Terminal.QuitError`.

var quitErrorTag = Tag("Terminal.QuitError.QuitError")

type ttyNode struct {
	mu    sync.Mutex
	ended bool
}

// rawUsers counts the open terminals: the first puts stdin in raw mode,
// the last puts it back.
var (
	rawMu    sync.Mutex
	rawUsers int
	rawUndo  func()
)

func openTty() *ttyNode {
	rawMu.Lock()
	if rawUsers == 0 {
		rawUndo = ttyMakeRaw()
	}
	rawUsers++
	rawMu.Unlock()
	return &ttyNode{}
}

func (t *ttyNode) close() {
	rawMu.Lock()
	if rawUsers--; rawUsers == 0 && rawUndo != nil {
		rawUndo()
		rawUndo = nil
	}
	rawMu.Unlock()
}

func (t *ttyNode) readKey(fb *Fiber) Value {
	t.mu.Lock()
	ended := t.ended
	t.mu.Unlock()
	if ended {
		return Fail(Nullary(quitErrorTag))
	}
	k, ok := stdinKeys.next(fb)
	if !ok || k.shouldQuit() {
		t.mu.Lock()
		t.ended = true
		t.mu.Unlock()
	}
	if !ok {
		return Fail(Nullary(quitErrorTag))
	}
	return userInputOf(k)
}

func (t *ttyNode) display(_ *Fiber, text string) { StdoutWrite(text) }

func (t *ttyNode) columns(*Fiber) int {
	columns, _, _ := windowSize(os.Stdout.Fd())
	return columns
}

func (t *ttyNode) rows() int {
	_, rows, _ := windowSize(os.Stdout.Fd())
	return rows
}

// PromptNode is `Cli.Prompt.node`: the terminal of the process, in raw
// mode until the scope of the layer closes.
func PromptNode(key Value) Value {
	return memoized(func(fb *Fiber, _ *MemoMap, scope *Owner) Services {
		t := openTty()
		scope.add(fb, func(*Fiber, *Cause) Value {
			t.close()
			return Unit
		})
		impl := Rec(
			"read_key", Fn(func(Value) Value { return Eff(t.readKey) }),
			"display", Fn(func(text Value) Value {
				return Eff(func(fb *Fiber) Value {
					t.display(fb, text.(string))
					return Unit
				})
			}),
			"columns", Fn(func(Value) Value { return Eff(func(fb *Fiber) Value { return int32(t.columns(fb)) }) }),
			"rows", Fn(func(Value) Value { return Eff(func(*Fiber) Value { return int32(t.rows()) }) }),
		)
		return Services{key.(string): impl}
	})
}
