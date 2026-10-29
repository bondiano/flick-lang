package flickrt

import (
	"sort"
	"strconv"
	"strings"
	"sync"
	"unicode"
)

// Normalization and case mapping as JS computes them, from the tables in
// unicode_tables.go, which tools/go-unicode/gen.mjs writes from Node.

type unicodeTables struct {
	nfd, nfkd, upper, lower map[rune][]rune
	composite               map[[2]rune]rune
	class                   map[rune]int
}

var (
	tablesOnce sync.Once
	tables     unicodeTables
)

func parseRunes(s string) []rune {
	var out []rune
	for _, h := range strings.Fields(s) {
		n, _ := strconv.ParseUint(h, 16, 32)
		out = append(out, rune(n))
	}
	return out
}

func parseMapping(data string) map[rune][]rune {
	out := map[rune][]rune{}
	for _, entry := range strings.Split(data, ";") {
		k, v, _ := strings.Cut(entry, "=")
		out[parseRunes(k)[0]] = parseRunes(v)
	}
	return out
}

func unicodeData() *unicodeTables {
	tablesOnce.Do(func() {
		tables = unicodeTables{
			nfd:       parseMapping(nfdData),
			nfkd:      parseMapping(nfkdData),
			upper:     parseMapping(upperData),
			lower:     parseMapping(lowerData),
			composite: map[[2]rune]rune{},
			class:     map[rune]int{},
		}
		for _, entry := range strings.Split(compositeData, ";") {
			k, v, _ := strings.Cut(entry, "=")
			pair := parseRunes(k)
			tables.composite[[2]rune{pair[0], pair[1]}] = parseRunes(v)[0]
		}
		for _, entry := range strings.Split(classData, ";") {
			k, v, _ := strings.Cut(entry, "=")
			rank, _ := strconv.Atoi(v)
			tables.class[parseRunes(k)[0]] = rank
		}
	})
	return &tables
}

const (
	hangulBase   = 0xAC00
	hangulCount  = 11172
	leadBase     = 0x1100
	vowelBase    = 0x1161
	trailBase    = 0x11A7
	vowelCount   = 21
	trailCount   = 28
	hangulPerRun = vowelCount * trailCount
)

// decomposeRunes is the full decomposition of `s` in canonical order.
func decomposeRunes(s string, compat bool) []rune {
	t := unicodeData()
	table := t.nfd
	if compat {
		table = t.nfkd
	}
	var out []rune
	for len(s) > 0 {
		r, n := decode(s)
		s = s[n:]
		switch d, ok := table[r]; {
		case r >= hangulBase && r < hangulBase+hangulCount:
			i := r - hangulBase
			out = append(out, leadBase+i/hangulPerRun, vowelBase+i%hangulPerRun/trailCount)
			if i%trailCount != 0 {
				out = append(out, trailBase+i%trailCount)
			}
		case ok:
			out = append(out, d...)
		default:
			out = append(out, r)
		}
	}
	// a stable sort of each run of nonstarters by class
	for i := 0; i < len(out); {
		if t.class[out[i]] == 0 {
			i++
			continue
		}
		j := i
		for j < len(out) && t.class[out[j]] != 0 {
			j++
		}
		run := out[i:j]
		sort.SliceStable(run, func(a, b int) bool { return t.class[run[a]] < t.class[run[b]] })
		i = j
	}
	return out
}

func compose(rs []rune) []rune {
	t := unicodeData()
	if len(rs) == 0 {
		return rs
	}
	out := []rune{rs[0]}
	starter := 0
	if t.class[rs[0]] != 0 {
		starter = -1
	}
	last := t.class[rs[0]]
	for _, r := range rs[1:] {
		c := t.class[r]
		if starter >= 0 {
			s := out[starter]
			// a mark blocked by one of the same or a higher class cannot join
			if last < c || last == 0 && len(out)-1 == starter {
				if lv, ok := hangulPair(s, r); ok {
					out[starter] = lv
					continue
				}
				if comp, ok := t.composite[[2]rune{s, r}]; ok {
					out[starter] = comp
					continue
				}
			}
		}
		if c == 0 {
			starter = len(out)
		}
		last = c
		out = append(out, r)
	}
	return out
}

func hangulPair(s, r rune) (rune, bool) {
	switch {
	case s >= leadBase && s < leadBase+19 && r >= vowelBase && r < vowelBase+vowelCount:
		return hangulBase + ((s-leadBase)*vowelCount+(r-vowelBase))*trailCount, true
	case s >= hangulBase && s < hangulBase+hangulCount && (s-hangulBase)%trailCount == 0 && r > trailBase && r < trailBase+trailCount:
		return s + (r - trailBase), true
	}
	return 0, false
}

func runesString(rs []rune) string {
	buf := make([]byte, 0, len(rs))
	for _, r := range rs {
		buf = encodeRune(buf, r)
	}
	return string(buf)
}

// Normalize is JS `s.normalize(form)` for the form a `NormalForm` names.
func Normalize(form, s Value) Value {
	text := s.(string)
	switch ConstructorName(form.(*Data).Tag) {
	case "NFD":
		return runesString(decomposeRunes(text, false))
	case "NFKD":
		return runesString(decomposeRunes(text, true))
	case "NFKC":
		return runesString(compose(decomposeRunes(text, true)))
	}
	return runesString(compose(decomposeRunes(text, false)))
}

func mapCase(s string, table map[rune][]rune, special func(rs []rune, i int) ([]rune, bool)) string {
	rs := []rune{}
	for len(s) > 0 {
		r, n := decode(s)
		rs = append(rs, r)
		s = s[n:]
	}
	var out []rune
	for i, r := range rs {
		if special != nil {
			if m, ok := special(rs, i); ok {
				out = append(out, m...)
				continue
			}
		}
		if m, ok := table[r]; ok {
			out = append(out, m...)
		} else {
			out = append(out, r)
		}
	}
	return runesString(out)
}

func cased(r rune) bool { return unicode.IsUpper(r) || unicode.IsLower(r) || unicode.IsTitle(r) }

// caseIgnorable approximates Unicode's Case_Ignorable for Final_Sigma.
func caseIgnorable(r rune) bool {
	return unicode.In(r, unicode.Mn, unicode.Me, unicode.Cf, unicode.Lm, unicode.Sk) ||
		strings.ContainsRune("'.:·‘’․‧︓﹒﹕＇．：", r)
}

// finalSigma is ς for a Σ that ends a word, as JS lower-cases it.
func finalSigma(rs []rune, i int) ([]rune, bool) {
	if rs[i] != 'Σ' {
		return nil, false
	}
	before := false
	for j := i - 1; j >= 0; j-- {
		if !caseIgnorable(rs[j]) {
			before = cased(rs[j])
			break
		}
	}
	after := false
	for j := i + 1; j < len(rs); j++ {
		if !caseIgnorable(rs[j]) {
			after = cased(rs[j])
			break
		}
	}
	if before && !after {
		return []rune{'ς'}, true
	}
	return nil, false
}

// ToUpperCase and ToLowerCase are JS's full case mappings.
func ToUpperCase(s Value) Value { return mapCase(s.(string), unicodeData().upper, nil) }
func ToLowerCase(s Value) Value { return mapCase(s.(string), unicodeData().lower, finalSigma) }

func turkic(locale string) bool {
	tag := strings.ToLower(strings.SplitN(strings.ReplaceAll(locale, "_", "-"), "-", 2)[0])
	return tag == "tr" || tag == "az"
}

// ToLocaleUpperCase and ToLowerCase follow the root mapping, but for
// Turkish and Azeri dotted and dotless i.
//
// shortcut: Lithuanian's dot rules use the root mapping; they need the locale's special casing
func ToLocaleUpperCase(locale, s Value) Value {
	if !turkic(locale.(string)) {
		return ToUpperCase(s)
	}
	return mapCase(s.(string), unicodeData().upper, func(rs []rune, i int) ([]rune, bool) {
		if rs[i] == 'i' {
			return []rune{'İ'}, true
		}
		return nil, false
	})
}

func ToLocaleLowerCase(locale, s Value) Value {
	if !turkic(locale.(string)) {
		return ToLowerCase(s)
	}
	return mapCase(s.(string), unicodeData().lower, func(rs []rune, i int) ([]rune, bool) {
		switch rs[i] {
		case 'İ':
			return []rune{'i'}, true
		case 'I':
			return []rune{'ı'}, true
		}
		return finalSigma(rs, i)
	})
}

// Capitalize upper-cases the first code unit, as Effect does.
func Capitalize(s Value) Value { return mapFirst(s.(string), unicodeData().upper) }
func Uncapitalize(s Value) Value {
	return mapFirst(s.(string), unicodeData().lower)
}

func mapFirst(s string, table map[rune][]rune) string {
	if s == "" {
		return s
	}
	r, n := decode(s)
	if r >= 0x10000 || r >= 0xD800 && r <= 0xDFFF {
		return s
	}
	if m, ok := table[r]; ok {
		return runesString(m) + s[n:]
	}
	return s
}
