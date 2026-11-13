package flickrt

import (
	"regexp"
	"strings"
	"unicode/utf8"
)

// Key presses as Node's readline decodes them from a terminal's input
// (`emitKeypressEvents`): a key is decoded from the characters it sent, an
// escape sequence named as Node names it, so prompts see the same keys on
// both targets.

// ttyKey is a decoded key press: the text it typed (none for an escape
// sequence) and the key.
type ttyKey struct {
	input                   string
	hasInput                bool
	name                    string
	named                   bool // Node sets the name; "" when it leaves it undefined
	ctrl, meta, shift, emit bool
}

// ttyTimeout is what the decoder gets when no character followed a lone
// escape within Node's `escapeCodeTimeout`.
const ttyTimeout = ""

const ttyEscape = "\x1b"

var (
	ttyTildeCmd  = regexp.MustCompile(`^(?:(\d\d?)(?:;(\d))?([~^$])|(\d{3}~))$`)
	ttyLetterCmd = regexp.MustCompile(`^((\d;)?(\d))?([A-Za-z])$`)
	ttyAlnum     = regexp.MustCompile(`^[0-9A-Za-z]$`)
)

// ttyCodes are the names of escape sequences, as Node's `emitKeys`.
var ttyCodes = map[string]string{
	"[P": "f1", "[Q": "f2", "[R": "f3", "[S": "f4",
	"OP": "f1", "OQ": "f2", "OR": "f3", "OS": "f4",
	"[11~": "f1", "[12~": "f2", "[13~": "f3", "[14~": "f4",
	"[200~": "paste-start", "[201~": "paste-end",
	"[[A": "f1", "[[B": "f2", "[[C": "f3", "[[D": "f4", "[[E": "f5",
	"[15~": "f5", "[17~": "f6", "[18~": "f7", "[19~": "f8",
	"[20~": "f9", "[21~": "f10", "[23~": "f11", "[24~": "f12",
	"[A": "up", "[B": "down", "[C": "right", "[D": "left", "[E": "clear", "[F": "end", "[H": "home",
	"OA": "up", "OB": "down", "OC": "right", "OD": "left", "OE": "clear", "OF": "end", "OH": "home",
	"[1~": "home", "[2~": "insert", "[3~": "delete", "[4~": "end", "[5~": "pageup", "[6~": "pagedown",
	"[[5~": "pageup", "[[6~": "pagedown",
	"[7~": "home", "[8~": "end",
}

// ttyModified are the sequences that carry their modifier in the code.
var ttyModified = map[string]struct {
	name        string
	shift, ctrl bool
}{
	"[a": {"up", true, false}, "[b": {"down", true, false}, "[c": {"right", true, false},
	"[d": {"left", true, false}, "[e": {"clear", true, false},
	"[2$": {"insert", true, false}, "[3$": {"delete", true, false}, "[5$": {"pageup", true, false},
	"[6$": {"pagedown", true, false}, "[7$": {"home", true, false}, "[8$": {"end", true, false},
	"Oa": {"up", false, true}, "Ob": {"down", false, true}, "Oc": {"right", false, true},
	"Od": {"left", false, true}, "Oe": {"clear", false, true},
	"[2^": {"insert", false, true}, "[3^": {"delete", false, true}, "[5^": {"pageup", false, true},
	"[6^": {"pagedown", false, true}, "[7^": {"home", false, true}, "[8^": {"end", false, true},
	"[Z": {"tab", true, false},
}

// ttyDecodeKey decodes the key at the front of `chars`, the characters of
// the input so far: how many it used, or ok false when the key needs more.
// A result with `emit` false is a broken sequence Node drops.
func ttyDecodeKey(chars []string) (k ttyKey, used int, ok bool) {
	i := 0
	next := func() (string, bool) {
		if i >= len(chars) {
			return "", false
		}
		i++
		return chars[i-1], true
	}
	ch, more := next()
	if !more {
		return k, 0, false
	}
	s := ch
	escaped := false
	if ch == ttyEscape {
		escaped = true
		if ch, more = next(); !more {
			return k, 0, false
		}
		s += ch
		if ch == ttyEscape {
			if ch, more = next(); !more {
				return k, 0, false
			}
			s += ch
		}
	}
	digit := func(c string) bool { return c >= "0" && c <= "9" }
	switch {
	case escaped && (ch == "O" || ch == "["):
		code := ch
		modifier := 0
		if ch == "O" {
			if ch, more = next(); !more {
				return k, 0, false
			}
			s += ch
			if digit(ch) {
				modifier = int(ch[0]-'0') - 1
				if ch, more = next(); !more {
					return k, 0, false
				}
				s += ch
			}
			code += ch
		} else {
			if ch, more = next(); !more {
				return k, 0, false
			}
			s += ch
			if ch == "[" {
				code += ch
				if ch, more = next(); !more {
					return k, 0, false
				}
				s += ch
			}
			cmdStart := len(s) - len(ch)
			for n := 0; n < 3 && digit(ch); n++ {
				if ch, more = next(); !more {
					return k, 0, false
				}
				s += ch
			}
			if ch == ";" {
				if ch, more = next(); !more {
					return k, 0, false
				}
				s += ch
				if digit(ch) {
					c, more := next()
					if !more {
						return k, 0, false
					}
					s += c
				}
			}
			cmd := s[cmdStart:]
			if m := ttyTildeCmd.FindStringSubmatch(cmd); m != nil {
				if m[4] != "" {
					code += m[4]
				} else {
					code += m[1] + m[3]
					modifier = 0
					if m[2] != "" {
						modifier = int(m[2][0]-'0') - 1
					}
				}
			} else if m := ttyLetterCmd.FindStringSubmatch(cmd); m != nil {
				code += m[4]
				modifier = 0
				if m[3] != "" {
					modifier = int(m[3][0]-'0') - 1
				}
			} else {
				code += cmd
			}
		}
		k.ctrl, k.meta, k.shift = modifier&4 != 0, modifier&10 != 0, modifier&1 != 0
		k.named = true
		if name, found := ttyCodes[code]; found {
			k.name = name
		} else if m, found := ttyModified[code]; found {
			k.name = m.name
			k.shift = k.shift || m.shift
			k.ctrl = k.ctrl || m.ctrl
		} else {
			k.name = "undefined"
		}
	case ch == "\r":
		k.name, k.named, k.meta = "return", true, escaped
	case ch == "\n":
		k.name, k.named, k.meta = "enter", true, escaped
	case ch == "\t":
		k.name, k.named, k.meta = "tab", true, escaped
	case ch == "\b" || ch == "\x7f":
		k.name, k.named, k.meta = "backspace", true, escaped
	case ch == ttyEscape:
		k.name, k.named, k.meta = "escape", true, escaped
	case ch == " ":
		k.name, k.named, k.meta = "space", true, escaped
	case !escaped && ch <= "\x1a":
		// ctrl+letter; the empty character of a timeout reads as Node's NaN code
		c := byte(0x60)
		if ch != "" {
			c = ch[0] + 'a' - 1
		}
		k.name, k.named, k.ctrl = string(rune(c)), true, true
	case ttyAlnum.MatchString(ch):
		k.name, k.named = strings.ToLower(ch), true
		k.shift = ch >= "A" && ch <= "Z"
		k.meta = escaped
	case escaped:
		// an escape the timeout ended
		if ch == ttyTimeout {
			k.name, k.named = "escape", true
		}
		k.meta = true
	}
	switch {
	case s != "" && (k.named || escaped):
		k.emit = true
		if !escaped {
			k.input, k.hasInput = s, true
		}
	case utf8.RuneCountInString(s) == 1:
		// a single unnamed character, as "."
		k.emit, k.input, k.hasInput = true, s, true
	}
	return k, i, true
}

// ttyChars splits text into the characters Node's decoder iterates.
func ttyChars(text string) []string {
	out := make([]string, 0, len(text))
	for text != "" {
		_, n := utf8.DecodeRuneInString(text)
		out = append(out, text[:n])
		text = text[n:]
	}
	return out
}
