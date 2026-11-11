package flickrt

import (
	"sort"
	"strconv"
	"strings"
	"sync"
)

// Normalization and case mapping as JS computes them, from the tables in
// unicode_tables.go, which tools/go-unicode/gen.mjs writes from Node.

type unicodeTables struct {
	nfd, nfkd, upper, lower      map[rune][]rune
	composite                    map[[2]rune]rune
	class                        map[rune]int
	cased, ignorable, softDotted []runeRange
	greek                        map[rune]greekLetter
}

// runeRange is a run of code points of a property, from `lo` to `hi`.
type runeRange struct{ lo, hi rune }

// parseRanges reads `lo-hi` and `cp` entries in hex, apart by ";".
func parseRanges(data string) []runeRange {
	var out []runeRange
	for _, entry := range strings.Split(data, ";") {
		lo, hi, isRange := strings.Cut(entry, "-")
		if !isRange {
			hi = lo
		}
		out = append(out, runeRange{parseRunes(lo)[0], parseRunes(hi)[0]})
	}
	return out
}

func inRanges(ranges []runeRange, r rune) bool {
	i := sort.Search(len(ranges), func(i int) bool { return ranges[i].hi >= r })
	return i < len(ranges) && ranges[i].lo <= r
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
			nfd:        parseMapping(nfdData),
			nfkd:       parseMapping(nfkdData),
			upper:      parseMapping(upperData),
			lower:      parseMapping(lowerData),
			composite:  map[[2]rune]rune{},
			class:      map[rune]int{},
			cased:      parseRanges(casedData),
			ignorable:  parseRanges(ignorableData),
			softDotted: parseRanges(softDotted),
			greek:      map[rune]greekLetter{},
		}
		for _, entry := range strings.Split(greekData, ";") {
			k, v, _ := strings.Cut(entry, "=")
			capital, flags, _ := strings.Cut(v, ".")
			n, _ := strconv.Atoi(flags)
			tables.greek[parseRunes(k)[0]] = greekLetter{parseRunes(capital)[0], n}
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

// toRunes is the code points of `s`, a lone surrogate as itself.
func toRunes(s string) []rune {
	rs := []rune{}
	for len(s) > 0 {
		r, n := decode(s)
		rs = append(rs, r)
		s = s[n:]
	}
	return rs
}

func mapCase(s string, table map[rune][]rune, special func(rs []rune, i int) ([]rune, bool)) string {
	rs := toRunes(s)
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

func cased(r rune) bool         { return inRanges(unicodeData().cased, r) }
func caseIgnorable(r rune) bool { return inRanges(unicodeData().ignorable, r) }

// casedAfter is whether a cased letter follows rs[i] past case-ignorable
// ones, and casedBefore whether one precedes it: Final_Sigma's context.
func casedAfter(rs []rune, i int) bool {
	for j := i + 1; j < len(rs); j++ {
		if !caseIgnorable(rs[j]) {
			return cased(rs[j])
		}
	}
	return false
}

func casedBefore(rs []rune, i int) bool {
	for j := i - 1; j >= 0; j-- {
		if !caseIgnorable(rs[j]) {
			return cased(rs[j])
		}
	}
	return false
}

// finalSigma is ς for a Σ that ends a word, as JS lower-cases it.
func finalSigma(rs []rune, i int) ([]rune, bool) {
	if rs[i] == 'Σ' && casedBefore(rs, i) && !casedAfter(rs, i) {
		return []rune{'ς'}, true
	}
	return nil, false
}

// ToUpperCase and ToLowerCase are JS's full case mappings.
// A string of ASCII has only its letters to map, each to one letter.
func ToUpperCase(s Value) Value {
	if text := s.(string); ascii(text) {
		return strings.ToUpper(text)
	}
	return mapCase(s.(string), unicodeData().upper, nil)
}

func ToLowerCase(s Value) Value {
	if text := s.(string); ascii(text) {
		return strings.ToLower(text)
	}
	return mapCase(s.(string), unicodeData().lower, finalSigma)
}

// Capitalize upper-cases the first code unit.
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
