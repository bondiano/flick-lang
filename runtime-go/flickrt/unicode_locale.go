package flickrt

// Case mapping by locale as V8 does it: ICU's rules for Turkish and Azeri
// (dotted and dotless i), Lithuanian (a dot above an accented i) and
// Greek upper case (without accents); other locales map as the root does.

// caseLocale is the language of a tag whose case mapping differs from the
// root's, or "", after throwing V8's RangeError for an invalid tag.
func caseLocale(tag string) string {
	switch l := parseLocale(tag).language; l {
	case "tr", "az", "lt", "el":
		return l
	}
	return ""
}

// ToLocaleUpperCase and ToLocaleLowerCase are JS's `toLocaleUpperCase` and
// `toLocaleLowerCase`.
func ToLocaleUpperCase(locale, s Value) Value {
	text, t := s.(string), unicodeData()
	switch caseLocale(locale.(string)) {
	case "el":
		return greekUpper(text)
	case "tr", "az":
		return mapCase(text, t.upper, func(rs []rune, i int) ([]rune, bool) {
			if rs[i] == 'i' {
				return []rune{'İ'}, true
			}
			return nil, false
		})
	case "lt":
		return mapCase(text, t.upper, func(rs []rune, i int) ([]rune, bool) {
			// a dot above after an i goes, as the capital has none
			return nil, rs[i] == 0x307 && precededBy(rs, i, func(r rune) bool { return dotType(r) == softDot })
		})
	}
	return ToUpperCase(s)
}

func ToLocaleLowerCase(locale, s Value) Value {
	text, t := s.(string), unicodeData()
	switch caseLocale(locale.(string)) {
	case "tr", "az":
		return mapCase(text, t.lower, func(rs []rune, i int) ([]rune, bool) {
			isI := func(r rune) bool { return r == 'I' }
			isDot := func(r rune) bool { return r == 0x307 }
			switch {
			case rs[i] == 'İ':
				return []rune{'i'}, true
			case rs[i] == 0x307 && precededBy(rs, i, isI):
				return nil, true
			case rs[i] == 'I' && !followedBy(rs, i, isDot):
				return []rune{'ı'}, true
			}
			return finalSigma(rs, i)
		})
	case "lt":
		return mapCase(text, t.lower, func(rs []rune, i int) ([]rune, bool) {
			above := func(r rune) bool { return dotType(r) == dotAbove }
			switch r := rs[i]; {
			case (r == 'I' || r == 'J' || r == 'Į') && followedBy(rs, i, above):
				return append(t.lower[r][:1:1], 0x307), true
			case r == 'Ì' || r == 'Í' || r == 'Ĩ':
				return []rune{'i', 0x307, t.nfd[r][1]}, true
			}
			return finalSigma(rs, i)
		})
	}
	return ToLowerCase(s)
}

// the dot types of ICU's case mapping
const (
	noDot       = iota
	softDot     // Soft_Dotted: i, j, į …
	dotAbove    // combining class 230
	otherAccent // another nonzero combining class
)

func dotType(r rune) int {
	t := unicodeData()
	switch c := t.class[r]; {
	case inRanges(t.softDotted, r):
		return softDot
	case c == 0:
		return noDot
	case c == t.class[0x301]:
		return dotAbove
	}
	return otherAccent
}

// precededBy is whether `want` takes the nearest rune before rs[i] that is
// not an other accent, and followedBy the nearest after it.
func precededBy(rs []rune, i int, want func(rune) bool) bool {
	for j := i - 1; j >= 0; j-- {
		if want(rs[j]) {
			return true
		}
		if dotType(rs[j]) != otherAccent {
			return false
		}
	}
	return false
}

func followedBy(rs []rune, i int, want func(rune) bool) bool {
	for j := i + 1; j < len(rs); j++ {
		if want(rs[j]) {
			return true
		}
		if dotType(rs[j]) != otherAccent {
			return false
		}
	}
	return false
}

// greekLetter is a letter of ICU's Greek upper case: its capital, without
// diacritics, and the greek* flags of the letter.
type greekLetter struct {
	capital rune
	flags   int
}

const (
	greekVowel = 1 << iota
	greekAccent
	greekDialytika
	greekYpogegrammeni
	greekCombiningDialytika
	greekOther
)

func greekDiacritic(r rune) int {
	switch r {
	case 0x300, 0x301, 0x342, 0x302, 0x303, 0x311:
		return greekAccent
	case 0x308:
		return greekCombiningDialytika
	case 0x344:
		return greekCombiningDialytika | greekAccent
	case 0x345:
		return greekYpogegrammeni
	case 0x304, 0x306, 0x313, 0x314, 0x343:
		return greekOther
	}
	return 0
}

// greekUpper is ICU's Greek upper case: a letter loses its accents and
// breathings and an iota subscript becomes a capital iota; a dialytika
// stays, and an iota or upsilon after an accented vowel gains one; and a
// lone eta keeps its tonos, as the disjunctive "or".
func greekUpper(s string) string {
	t := unicodeData()
	rs := toRunes(s)
	var out []rune
	afterCased, afterAccent := false, 0
	for i := 0; i < len(rs); {
		r, next := rs[i], i+1
		nextCased, nextAccent := cased(r), 0
		if caseIgnorable(r) {
			nextCased = afterCased
		}
		g, ok := t.greek[r]
		if !ok {
			if m, ok := t.upper[r]; ok {
				out = append(out, m...)
			} else {
				out = append(out, r)
			}
			afterCased, afterAccent, i = nextCased, 0, next
			continue
		}
		data, capital := g.flags, g.capital
		if data&greekVowel != 0 && (capital == 'Ι' || capital == 'Υ') {
			data |= afterAccent
		}
		iotas, precomposed := 0, data&greekAccent != 0
		if data&greekYpogegrammeni != 0 {
			iotas = 1
		}
		for ; next < len(rs) && greekDiacritic(rs[next]) != 0; next++ {
			if data |= greekDiacritic(rs[next]); rs[next] == 0x345 {
				iotas++
			}
		}
		if data&(greekVowel|greekAccent|greekDialytika) == greekVowel|greekAccent {
			nextAccent = greekCombiningDialytika
			if precomposed {
				nextAccent = greekDialytika
			}
		}
		tonos := false
		if capital == 'Η' && data&greekAccent != 0 && iotas == 0 && !afterCased && !casedAfter(rs, next-1) {
			if precomposed {
				capital = 'Ή'
			} else {
				tonos = true
			}
		} else if data&greekDialytika != 0 && (capital == 'Ι' || capital == 'Υ') {
			capital = map[rune]rune{'Ι': 'Ϊ', 'Υ': 'Ϋ'}[capital]
			data &^= greekDialytika | greekCombiningDialytika
		}
		out = append(out, capital)
		if data&(greekDialytika|greekCombiningDialytika) != 0 {
			out = append(out, 0x308)
		}
		if tonos {
			out = append(out, 0x301)
		}
		for ; iotas > 0; iotas-- {
			out = append(out, 'Ι')
		}
		afterCased, afterAccent, i = nextCased, nextAccent, next
	}
	return runesString(out)
}
