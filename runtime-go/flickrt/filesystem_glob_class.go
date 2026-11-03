package flickrt

// The bracket classes of Effect's internal/glob.ts: `[...]` with `!` or `^`
// negation, ranges, `\` escapes and POSIX classes, over code points.

type fsGlobTokenKind uint8

const (
	fsGlobChar fsGlobTokenKind = iota
	fsGlobAny
	fsGlobStar
	fsGlobClass
)

// fsGlobToken is one token of a wildcard segment; `ranges` holds inclusive
// code point pairs of a class.
type fsGlobToken struct {
	kind    fsGlobTokenKind
	code    rune
	ranges  []rune
	negated bool
}

var fsGlobPosixClasses = map[string][]rune{
	"alnum":  {'0', '9', 'A', 'Z', 'a', 'z'},
	"alpha":  {'A', 'Z', 'a', 'z'},
	"blank":  {0x09, 0x09, 0x20, 0x20},
	"cntrl":  {0x00, 0x1f, 0x7f, 0x7f},
	"digit":  {'0', '9'},
	"graph":  {0x21, 0x7e},
	"lower":  {'a', 'z'},
	"print":  {0x20, 0x7e},
	"punct":  {0x21, 0x2f, 0x3a, 0x40, 0x5b, 0x60, 0x7b, 0x7e},
	"space":  {0x09, 0x0d, 0x20, 0x20},
	"upper":  {'A', 'Z'},
	"word":   {'0', '9', 'A', 'Z', 0x5f, 0x5f, 'a', 'z'},
	"xdigit": {'0', '9', 'A', 'F', 'a', 'f'},
}

// fsGlobParsePosix is parsePosixClass: the class `[:name:]` at `open`,
// whose name is at most six characters, and the index of its `]`.
func fsGlobParsePosix(chars []rune, open int) ([]rune, int, bool) {
	for i := open + 2; i < len(chars) && i <= open+8; i++ {
		if chars[i] == ':' && i+1 < len(chars) && chars[i+1] == ']' {
			ranges, ok := fsGlobPosixClasses[string(chars[open+2:i])]
			return ranges, i + 1, ok
		}
	}
	return nil, 0, false
}

// fsGlobParseClass is parseClass: the class opening at `open` and the index
// of its `]`, or false when it does not close within the segment.
func fsGlobParseClass(chars []rune, open int) (fsGlobToken, int, bool) {
	i := open + 1
	negated := i < len(chars) && (chars[i] == '!' || chars[i] == '^')
	if negated {
		i++
	}
	ranges := []rune{}
	for first := true; i < len(chars); i, first = i+1, false {
		char := chars[i]
		if char == '/' {
			return fsGlobToken{}, 0, false
		}
		if char == ']' && !first {
			return fsGlobToken{kind: fsGlobClass, ranges: ranges, negated: negated}, i, true
		}
		if char == '[' && i+1 < len(chars) && chars[i+1] == ':' {
			if posix, end, ok := fsGlobParsePosix(chars, i); ok {
				ranges = append(ranges, posix...)
				i = end
				continue
			}
		}
		if char == '\\' && i+1 < len(chars) {
			i++
			char = chars[i]
		}
		if i+2 < len(chars) && chars[i+1] == '-' && chars[i+2] != ']' {
			next := i + 2
			if chars[next] == '/' {
				return fsGlobToken{}, 0, false
			}
			if chars[next] == '\\' && next+1 < len(chars) {
				next++
			}
			ranges = append(ranges, char, chars[next])
			i = next
			continue
		}
		ranges = append(ranges, char, char)
	}
	return fsGlobToken{}, 0, false
}

func (token fsGlobToken) match(code rune) bool {
	switch token.kind {
	case fsGlobChar:
		return token.code == code
	case fsGlobAny:
		return true
	case fsGlobClass:
		found := false
		for i := 0; i < len(token.ranges) && !found; i += 2 {
			found = code >= token.ranges[i] && code <= token.ranges[i+1]
		}
		return found != token.negated
	}
	return false
}
