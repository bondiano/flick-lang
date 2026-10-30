package flickrt

import (
	"strings"
	"unicode"
)

type fsGlobRange struct{ lo, hi rune }

type fsGlobClass struct {
	ranges []fsGlobRange
	posix  []string
	negate bool
}

func fsGlobParseClass(pattern string) (fsGlobToken, int, bool) {
	class := &fsGlobClass{}
	i := 1
	if i < len(pattern) && (pattern[i] == '!' || pattern[i] == '^') {
		class.negate = true
		i++
	}
	start, closed := i, false
	for i < len(pattern) {
		if pattern[i] == ']' && i > start {
			i++
			closed = true
			break
		}
		if strings.HasPrefix(pattern[i:], "[:") {
			if end := strings.Index(pattern[i+2:], ":]"); end >= 0 {
				kind := pattern[i+2 : i+2+end]
				if strings.Contains("|alnum|alpha|ascii|blank|cntrl|digit|graph|lower|print|punct|space|upper|word|xdigit|", "|"+kind+"|") {
					class.posix = append(class.posix, kind)
					i += end + 4
					continue
				}
			}
		}
		lo, size := fsGlobRune(pattern[i:])
		i += size
		hi := lo
		if i+1 < len(pattern) && pattern[i] == '-' && pattern[i+1] != ']' {
			hi, size = fsGlobRune(pattern[i+1:])
			i += 1 + size
		}
		if lo <= hi {
			class.ranges = append(class.ranges, fsGlobRange{lo, hi})
		}
	}
	if !closed {
		return fsGlobToken{}, 0, false
	}
	if !class.negate && len(class.posix) == 0 && len(class.ranges) == 1 && class.ranges[0].lo == class.ranges[0].hi {
		return fsGlobToken{op: '=', char: class.ranges[0].lo}, i, false
	}
	return fsGlobToken{op: '[', class: class}, i, true
}

func (class *fsGlobClass) match(ch rune, nocase, unicodeCase bool) bool {
	if len(class.ranges) == 0 && len(class.posix) == 0 {
		// minimatch emits the never-matching `$.` for a class with nothing in
		// it, even when negated (`[!z-a]`).
		return false
	}
	for _, interval := range class.ranges {
		if ch >= interval.lo && ch <= interval.hi || nocase && fsGlobFoldInRange(ch, interval.lo, interval.hi, unicodeCase) {
			return !class.negate
		}
	}
	if len(class.posix) != 0 {
		current := ch
		for {
			for _, kind := range class.posix {
				if fsGlobPosixClass(kind, current) {
					return !class.negate
				}
			}
			if !nocase || !unicodeCase {
				break
			}
			current = unicode.SimpleFold(current)
			if current == ch {
				break
			}
		}
	}
	return class.negate
}

// fsGlobFoldInRange tests the case-fold orbit of `ch` against the original
// interval; canonicalization is not monotone, so the endpoints cannot be
// folded. Non-Unicode members must canonicalize to the same value (the ASCII
// guard), while Unicode (`/iu`) uses the simple fold orbit directly.
func fsGlobFoldInRange(ch, lo, hi rune, unicodeCase bool) bool {
	target := fsGlobCanonical(ch)
	current := ch
	for range 256 {
		if current >= lo && current <= hi && (unicodeCase || fsGlobCanonical(current) == target) {
			return true
		}
		next := unicode.SimpleFold(current)
		if next == ch {
			return false
		}
		current = next
	}
	return false
}

// fsGlobEqual is JS case-insensitive equality: `EqualFold` for the Unicode
// (`/iu`) form, non-Unicode Canonicalize otherwise.
func fsGlobEqual(a, b rune, nocase, unicodeCase bool) bool {
	if a == b {
		return true
	}
	if !nocase {
		return false
	}
	if unicodeCase {
		return strings.EqualFold(string(a), string(b))
	}
	return fsGlobCanonical(a) == fsGlobCanonical(b)
}

// fsGlobCanonical is ES's non-Unicode Canonicalize: upper case only when the
// single-code-unit result stays non-ASCII; non-ASCII never folds into ASCII.
func fsGlobCanonical(ch rune) rune {
	if ch < 128 {
		return unicode.ToUpper(ch)
	}
	upper, ok := unicodeData().upper[ch]
	if !ok || len(upper) != 1 || upper[0] < 128 || upper[0] > 0xffff {
		return ch
	}
	return upper[0]
}

func fsGlobPosixClass(kind string, ch rune) bool {
	switch kind {
	case "alnum":
		return unicode.IsLetter(ch) || unicode.Is(unicode.Nl, ch) || unicode.IsDigit(ch)
	case "alpha":
		return unicode.IsLetter(ch) || unicode.Is(unicode.Nl, ch)
	case "ascii":
		return ch <= 127
	case "blank":
		return unicode.Is(unicode.Zs, ch) || ch == '\t'
	case "cntrl":
		return unicode.IsControl(ch)
	case "digit":
		return unicode.IsDigit(ch)
	case "graph":
		return unicode.IsGraphic(ch) && !unicode.Is(unicode.Zs, ch)
	case "lower":
		return unicode.IsLower(ch)
	case "print":
		// Node's print is \p{C}, including Cn missing from Go 1.23's table.
		return !unicode.IsGraphic(ch) && !unicode.Is(unicode.Z, ch)
	case "punct":
		return unicode.IsPunct(ch)
	case "space":
		return unicode.Is(unicode.Z, ch) || strings.ContainsRune("\t\r\n\v\f", ch)
	case "upper":
		return unicode.IsUpper(ch)
	case "word":
		return unicode.IsLetter(ch) || unicode.Is(unicode.Nl, ch) || unicode.IsDigit(ch) || unicode.Is(unicode.Pc, ch)
	case "xdigit":
		return ch >= '0' && ch <= '9' || ch >= 'a' && ch <= 'f' || ch >= 'A' && ch <= 'F'
	}
	return false
}
