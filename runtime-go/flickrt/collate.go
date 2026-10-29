package flickrt

import (
	"strconv"
	"strings"
	"sync"
)

// Collation as Node's Intl.Collator("en") orders strings, from the weights
// tools/go-unicode/gen.mjs writes: strings compare by their primary
// weights, then by their secondary ones (accents), then by their tertiary
// ones (case and variants).
//
// shortcut: every locale collates as "en" (the root order) and contractions are not weighed; tailor locales when a program needs one

type weight struct{ primary, secondary, tertiary int }

var (
	collationOnce sync.Once
	primaryOf     map[rune]weight
	secondaryOf   map[rune]int
	expansionOf   map[rune][]rune
)

func collation() {
	collationOnce.Do(func() {
		primaryOf, secondaryOf = map[rune]weight{}, map[rune]int{}
		for _, entry := range strings.Split(primaryData, ";") {
			k, v, _ := strings.Cut(entry, "=")
			p, t, _ := strings.Cut(v, ".")
			pn, _ := strconv.Atoi(p)
			tn, _ := strconv.Atoi(t)
			primaryOf[parseRunes(k)[0]] = weight{pn, 0, tn}
		}
		for _, entry := range strings.Split(secondaryData, ";") {
			k, v, _ := strings.Cut(entry, "=")
			n, _ := strconv.Atoi(v)
			secondaryOf[parseRunes(k)[0]] = n
		}
		expansionOf = parseMapping(expansionData)
	})
}

// expanded marks the weights of a letter that expands (ß as ss): after
// the letters it expands to at the secondary and tertiary levels.
const expanded = 1000

func weigh(s string) []weight {
	collation()
	var out []weight
	for _, r := range decomposeRunes(s, false) {
		parts, bump := []rune{r}, 0
		if e, ok := expansionOf[r]; ok {
			parts, bump = e, expanded
		}
		for _, c := range parts {
			if w, ok := primaryOf[c]; ok {
				out = append(out, weight{w.primary, bump, w.tertiary + bump})
			} else if n, ok := secondaryOf[c]; ok {
				out = append(out, weight{0, n, 0})
			} else if c >= collated {
				out = append(out, weight{1<<30 + int(c), 0, 0})
			}
		}
	}
	return out
}

func level(ws []weight, f func(w weight) (int, bool)) []int {
	var out []int
	for _, w := range ws {
		if n, ok := f(w); ok {
			out = append(out, n)
		}
	}
	return out
}

func compareInts(a, b []int) int {
	for i := 0; i < len(a) && i < len(b); i++ {
		if a[i] != b[i] {
			return a[i] - b[i]
		}
	}
	return len(a) - len(b)
}

func collate(a, b string) int {
	x, y := weigh(a), weigh(b)
	levels := []func(w weight) (int, bool){
		func(w weight) (int, bool) { return w.primary, w.primary != 0 },
		func(w weight) (int, bool) { return w.secondary, w.primary != 0 || w.secondary != 0 },
		func(w weight) (int, bool) { return w.tertiary, w.primary != 0 },
	}
	for _, f := range levels {
		if c := compareInts(level(x, f), level(y, f)); c != 0 {
			return c
		}
	}
	return 0
}

// LocaleCompare is `self` against `that` as JS `localeCompare` orders them.
func LocaleCompare(_, that, self Value) Value { return ordering(collate(self.(string), that.(string))) }
