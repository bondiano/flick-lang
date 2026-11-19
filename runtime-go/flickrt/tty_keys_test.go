package flickrt

import (
	"strings"
	"testing"
	"time"
)

// nodeKey is a key press as Node 24's readline emits it: the text (none
// for an escape sequence), the name ("" for none) and the modifiers.
type nodeKey struct {
	input, name       string
	ctrl, meta, shift bool
}

// nodeKeys is what Node's `emitKeypressEvents` emits for each input, the
// escape timeout passed.
var nodeKeys = []struct {
	input    string
	keys     []nodeKey
	hasInput []bool
}{
	{"a", []nodeKey{{"a", "a", false, false, false}}, []bool{true}},
	{"A", []nodeKey{{"A", "a", false, false, true}}, []bool{true}},
	{"1", []nodeKey{{"1", "1", false, false, false}}, []bool{true}},
	{" ", []nodeKey{{" ", "space", false, false, false}}, []bool{true}},
	{"\x0d", []nodeKey{{"\x0d", "return", false, false, false}}, []bool{true}},
	{"\x0a", []nodeKey{{"\x0a", "enter", false, false, false}}, []bool{true}},
	{"\x09", []nodeKey{{"\x09", "tab", false, false, false}}, []bool{true}},
	{"\x7f", []nodeKey{{"\x7f", "backspace", false, false, false}}, []bool{true}},
	{"\x08", []nodeKey{{"\x08", "backspace", false, false, false}}, []bool{true}},
	{"\x03", []nodeKey{{"\x03", "c", true, false, false}}, []bool{true}},
	{"\x04", []nodeKey{{"\x04", "d", true, false, false}}, []bool{true}},
	{"\x00", []nodeKey{{"\x00", "`", true, false, false}}, []bool{true}},
	{"\x1c", []nodeKey{{"\x1c", "", false, false, false}}, []bool{true}},
	{".", []nodeKey{{".", "", false, false, false}}, []bool{true}},
	{"é", []nodeKey{{"é", "", false, false, false}}, []bool{true}},
	{"😀", []nodeKey{{"😀", "", false, false, false}}, []bool{true}},
	{"\x1b[A", []nodeKey{{"", "up", false, false, false}}, []bool{false}},
	{"\x1b[B", []nodeKey{{"", "down", false, false, false}}, []bool{false}},
	{"\x1b[C", []nodeKey{{"", "right", false, false, false}}, []bool{false}},
	{"\x1b[D", []nodeKey{{"", "left", false, false, false}}, []bool{false}},
	{"\x1b[H", []nodeKey{{"", "home", false, false, false}}, []bool{false}},
	{"\x1b[F", []nodeKey{{"", "end", false, false, false}}, []bool{false}},
	{"\x1bOA", []nodeKey{{"", "up", false, false, false}}, []bool{false}},
	{"\x1bOP", []nodeKey{{"", "f1", false, false, false}}, []bool{false}},
	{"\x1b[1;5C", []nodeKey{{"", "right", true, false, false}}, []bool{false}},
	{"\x1b[1;2A", []nodeKey{{"", "up", false, false, true}}, []bool{false}},
	{"\x1b[3~", []nodeKey{{"", "delete", false, false, false}}, []bool{false}},
	{"\x1b[5~", []nodeKey{{"", "pageup", false, false, false}}, []bool{false}},
	{"\x1b[3;5~", []nodeKey{{"", "delete", true, false, false}}, []bool{false}},
	{"\x1b[24~", []nodeKey{{"", "f12", false, false, false}}, []bool{false}},
	{"\x1b[200~", []nodeKey{{"", "paste-start", false, false, false}}, []bool{false}},
	{"\x1b[Z", []nodeKey{{"", "tab", false, false, true}}, []bool{false}},
	{"\x1b[a", []nodeKey{{"", "up", false, false, true}}, []bool{false}},
	{"\x1bOa", []nodeKey{{"", "up", true, false, false}}, []bool{false}},
	{"\x1b[2^", []nodeKey{{"", "insert", true, false, false}}, []bool{false}},
	{"\x1b[[A", []nodeKey{{"", "f1", false, false, false}}, []bool{false}},
	{"\x1b[[5~", []nodeKey{{"", "pageup", false, false, false}}, []bool{false}},
	{"\x1bb", []nodeKey{{"", "b", false, true, false}}, []bool{false}},
	{"\x1bB", []nodeKey{{"", "b", false, true, true}}, []bool{false}},
	{"\x1b\x0d", []nodeKey{{"", "return", false, true, false}}, []bool{false}},
	{"\x1b\x1b[A", []nodeKey{{"", "up", false, false, false}}, []bool{false}},
	{"\x1b[99z", []nodeKey{{"", "undefined", false, false, false}}, []bool{false}},
	{"\x1b[1;5", []nodeKey{}, []bool{}},
	{"\x1bé", []nodeKey{{"", "", false, true, false}}, []bool{false}},
	{"\x1b5", []nodeKey{{"", "5", false, true, false}}, []bool{false}}}

func ttyDecodeAll(input string) []ttyKey {
	r := newTtyReader(strings.NewReader(""))
	chars := ttyChars(input)
	if len(chars) > 0 && chars[len(chars)-1] == ttyEscape {
		chars = append(chars, ttyTimeout)
	}
	r.feed(chars)
	return r.keys
}

func TestKeysDecodeAsNode(t *testing.T) {
	for _, c := range nodeKeys {
		got := ttyDecodeAll(c.input)
		if len(got) != len(c.keys) {
			t.Errorf("%q: %d keys, want %d", c.input, len(got), len(c.keys))
			continue
		}
		for i, want := range c.keys {
			k := got[i]
			if k.name != want.name || k.ctrl != want.ctrl || k.meta != want.meta || k.shift != want.shift ||
				k.hasInput != c.hasInput[i] || (k.hasInput && k.input != want.input) {
				t.Errorf("%q: %+v, want %+v (input %v)", c.input, k, want, c.hasInput[i])
			}
		}
	}
}

// A key split across reads is decoded once it is whole, and a lone escape
// becomes the escape key after the timeout.
func TestTtyReaderJoinsReads(t *testing.T) {
	r := newTtyReader(strings.NewReader(""))
	r.feed(ttyChars("\x1b["))
	if len(r.keys) != 0 {
		t.Fatalf("keys of half a sequence: %+v", r.keys)
	}
	r.feed(ttyChars("Ax"))
	if len(r.keys) != 2 || r.keys[0].name != "up" || r.keys[1].input != "x" {
		t.Fatalf("keys %+v", r.keys)
	}
	if completeUTF8([]byte("a\xc3")) != 1 || completeUTF8([]byte("a\xc3\xa9")) != 3 {
		t.Fatal("completeUTF8 splits a character")
	}
	fb := rootFiber()
	esc := newTtyReader(strings.NewReader("\x1b"))
	started := time.Now()
	if k, ok := esc.next(fb); !ok || k.name != "escape" || !k.meta || k.hasInput {
		t.Fatalf("lone escape %+v %v", k, ok)
	}
	if time.Since(started) < 40*time.Millisecond {
		t.Fatal("the escape did not wait for its timeout")
	}
	if _, ok := esc.next(fb); ok {
		t.Fatal("a key after the end of the input")
	}
}
