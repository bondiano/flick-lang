package flickrt

import (
	"sort"
	"strconv"
	"strings"
	"sync"
)

// Collation as Node's Intl.Collator orders strings: the Unicode Collation
// Algorithm on the CLDR root collation and the tailorings of the languages
// of the 52 common locales, from the tables tools/go-collate/gen.mjs
// writes (collate_tables.go). Strings compare by their primary weights,
// then their secondary ones (accents), then their tertiary ones (case and
// variants), at Intl's default strength.
//
// shortcut: a tag of another language collates as the default locale, and Unicode extensions (`-u-co-phonebk`, `-u-kn`) are ignored; generate more tailorings when a program needs them

// collElem is a collation element: ICU's weights, left-aligned (a primary
// in 32 bits, the others in 16), shifted to leave room for the offset of
// a tailored weight from the root weight it follows or precedes.
type collElem struct {
	p    uint64 // primary << 20 + offset
	s, t uint32 // secondary, tertiary without case bits: << 12 + 2048 + offset
	c    uint8  // case bits: 0 lowercase, 1 mixed, 2 uppercase
}

const collCommon = 0x0500<<12 + 2048

// collMapping is the elements of a string, after `prefix` if it has one.
type collMapping struct {
	prefix []rune
	ces    []collElem
}

type collTable struct {
	single                map[rune][]collMapping
	multi                 map[string][]collMapping
	starts                map[rune]bool // the first code points of multi's keys
	maxKey                int
	backwards, upperFirst bool
	reorder               []byte // the new lead byte of each primary's, or nil
}

// collWeight reads a weight: hex bytes left-aligned to `width`, then an
// offset `+k` or `-k`.
func collWeight(text string, width int) (uint32, int) {
	sign := strings.IndexAny(text, "+-")
	if sign < 0 {
		sign = len(text)
	}
	v, _ := strconv.ParseUint(text[:sign]+strings.Repeat("0", 2*width-sign), 16, 32)
	k := int64(0)
	if sign < len(text) {
		k, _ = strconv.ParseInt(text[sign:], 16, 32)
	}
	return uint32(v), int(k)
}

func collElement(text string) collElem {
	fields := strings.Split(text, ".")
	p, kp := collWeight(fields[0], 4)
	out := collElem{p: uint64(p)<<20 + uint64(kp), s: collCommon, t: collCommon}
	if len(fields) > 1 {
		s, ks := collWeight(fields[1], 2)
		out.s = 0
		if s != 0 || ks != 0 {
			out.s = s<<12 + uint32(2048+ks)
		}
	}
	if len(fields) > 2 {
		t, kt := collWeight(fields[2], 2)
		out.c, t = uint8(t>>14), t&0x3f3f
		out.t = 0
		if t != 0 || kt != 0 {
			out.t = t<<12 + uint32(2048+kt)
		}
	}
	return out
}

func collRunes(text string) []rune {
	var out []rune
	for _, h := range strings.Split(text, ",") {
		n, _ := strconv.ParseUint(h, 16, 32)
		out = append(out, rune(n))
	}
	return out
}

func parseCollation(data string) *collTable {
	t := &collTable{single: map[rune][]collMapping{}, multi: map[string][]collMapping{}, starts: map[rune]bool{}, maxKey: 1}
	add := func(key []rune, m collMapping) {
		list := func(old []collMapping) []collMapping {
			// prefixed mappings first, the longest first
			out := append(old, m)
			sort.SliceStable(out, func(i, j int) bool { return len(out[i].prefix) > len(out[j].prefix) })
			return out
		}
		if len(key) == 1 {
			t.single[key[0]] = list(t.single[key[0]])
			return
		}
		t.multi[string(key)] = list(t.multi[string(key)])
		t.starts[key[0]] = true
		t.maxKey = max(t.maxKey, len(key))
	}
	for _, entry := range strings.Split(data, ";") {
		head, body, _ := strings.Cut(entry, ":")
		if run, ok := strings.CutPrefix(head, "@"); ok {
			p, k := collWeight(run, 4)
			for _, r := range body {
				add([]rune{r}, collMapping{ces: []collElem{{p: uint64(p)<<20 + uint64(k), s: collCommon, t: collCommon}}})
				k++
			}
			continue
		}
		var m collMapping
		if prefix, key, ok := strings.Cut(head, "|"); ok {
			m.prefix, head = collRunes(prefix), key
		}
		for _, ce := range strings.Fields(body) {
			m.ces = append(m.ces, collElement(ce))
		}
		add(collRunes(head), m)
	}
	return t
}

type hanRange struct {
	lo, hi rune
	rank   uint32
}

var (
	collRoot   onFirstUse[*collTable]
	hanRanges  onFirstUse[[]hanRange]
	collLines  onFirstUse[map[string]string]
	collTags   onFirstUse[map[string]string]
	collTables sync.Map // id → *collTable
)

// collImplicit is the element ICU gives a code point the tables lack: Han
// in radical-stroke order (hanData), then the others by code point.
func collImplicit(r rune) collElem {
	ranges := hanRanges.get(func() []hanRange {
		var out []hanRange
		rank, previous, inRange := uint32(0), rune(0), false
		for _, c := range hanData {
			switch {
			case c == '-':
				inRange = true
				continue
			case inRange:
				out = append(out, hanRange{previous + 1, c, rank})
				rank += uint32(c - previous)
			default:
				out = append(out, hanRange{c, c, rank})
				rank++
			}
			previous, inRange = c, false
		}
		sort.Slice(out, func(i, j int) bool { return out[i].lo < out[j].lo })
		return out
	})
	p := 0xe4000000 + uint64(r)
	if i := sort.Search(len(ranges), func(i int) bool { return ranges[i].hi >= r }); i < len(ranges) && ranges[i].lo <= r {
		p = uint64(0xe0000000 + ranges[i].rank + uint32(r-ranges[i].lo))
	}
	return collElem{p: p << 20, s: collCommon, t: collCommon}
}

// collator is a tailoring over the root, or the root alone.
type collator struct{ tail, root *collTable }

// collatorOf is the collation Intl negotiates for a tag: its own, or the
// default locale's, or the root.
func collatorOf(id localeID) collator {
	root := collRoot.get(func() *collTable { return parseCollation(collationRootData) })
	tags := collTags.get(func() map[string]string { return parseTagMap(collationLocaleData) })
	_, name, ok := lookupLocale(id, tags)
	if !ok {
		if d, problem := languageTag(defaultLocale()); problem == "" {
			_, name, ok = lookupLocale(d, tags)
		}
	}
	if !ok || name == "root" {
		return collator{root: root}
	}
	if t, ok := collTables.Load(name); ok {
		return collator{t.(*collTable), root}
	}
	lines := collLines.get(func() map[string]string {
		out := map[string]string{}
		for _, line := range strings.Split(collationData, "\n") {
			id, rest, _ := strings.Cut(line, "\t")
			out[id] = rest
		}
		return out
	})
	fields := strings.SplitN(lines[name], "\t", 3)
	t := parseCollation(fields[2])
	t.backwards, t.upperFirst = strings.Contains(fields[0], "b"), strings.Contains(fields[0], "u")
	for i := 0; i+2 <= len(fields[1]); i += 2 {
		b, _ := strconv.ParseUint(fields[1][i:i+2], 16, 8)
		t.reorder = append(t.reorder, byte(b))
	}
	loaded, _ := collTables.LoadOrStore(name, t)
	return collator{loaded.(*collTable), root}
}

func parseTagMap(data string) map[string]string {
	out := map[string]string{}
	for _, entry := range strings.Split(data, ";") {
		tag, id, _ := strings.Cut(entry, "=")
		out[tag] = id
	}
	return out
}

// find is the elements of a key after `before`: the tailoring's, else the root's.
func (c collator) find(key, before []rune) ([]collElem, bool) {
	for _, t := range [2]*collTable{c.tail, c.root} {
		if t == nil {
			continue
		}
		list := t.single[key[0]]
		if len(key) > 1 {
			list = t.multi[string(key)]
		}
		for _, m := range list {
			if len(m.prefix) <= len(before) && string(before[len(before)-len(m.prefix):]) == string(m.prefix) {
				return m.ces, true
			}
		}
	}
	return nil, false
}

func (c collator) contracts(r rune) bool {
	return c.root.starts[r] || c.tail != nil && c.tail.starts[r]
}

// elements is the collation elements of a string: of the longest keys,
// each extended by the unblocked nonstarters after it it contracts with.
func (c collator) elements(s string) []collElem {
	rs := decomposeRunes(s, false)
	class := unicodeData().class
	maxKey := c.root.maxKey
	if c.tail != nil {
		maxKey = max(maxKey, c.tail.maxKey)
	}
	var out []collElem
	var used []bool // nonstarters a discontiguous match took
	for i := 0; i < len(rs); {
		if used != nil && used[i] {
			i++
			continue
		}
		n, ces, found := 1, []collElem(nil), false
		if c.contracts(rs[i]) {
			for l := min(maxKey, len(rs)-i); l > 1 && !found; l-- {
				if !anyUsed(used, i, l) {
					n = l
					ces, found = c.find(rs[i:i+l], rs[:i])
				}
			}
		}
		if !found {
			n = 1
			ces, found = c.find(rs[i:i+1], rs[:i])
		}
		if found && c.contracts(rs[i]) {
			key, blocked := append([]rune(nil), rs[i:i+n]...), -1
			for j := i + n; j < len(rs) && class[rs[j]] != 0; j++ {
				if used != nil && used[j] {
					continue
				}
				if cc := class[rs[j]]; blocked < cc {
					if more, ok := c.find(append(key, rs[j]), rs[:i]); ok {
						key, ces = append(key, rs[j]), more
						if used == nil {
							used = make([]bool, len(rs))
						}
						used[j] = true
						continue
					}
				}
				blocked = max(blocked, class[rs[j]])
			}
		}
		if !found {
			ces = []collElem{collImplicit(rs[i])}
		}
		out = append(out, ces...)
		i += n
	}
	return out
}

// anyUsed is whether a discontiguous match took one of the n runes from i.
func anyUsed(used []bool, i, n int) bool {
	for _, u := range used[min(i, len(used)):min(i+n, len(used))] {
		if u {
			return true
		}
	}
	return false
}

// level is the weights of a level, in the order they compare.
func (c collator) level(ces []collElem, n int) []uint64 {
	t := c.root
	if c.tail != nil {
		t = c.tail
	}
	var out []uint64
	for _, e := range ces {
		switch {
		case n == 1 && e.p != 0:
			p := e.p
			if t.reorder != nil {
				lead := p >> 44 & 0xff
				p = p&^(0xff<<44) | uint64(t.reorder[lead])<<44
			}
			out = append(out, p)
		case n == 2 && e.s != 0:
			out = append(out, uint64(e.s))
		case n == 3 && e.t != 0:
			w := uint64(e.t)
			if t.upperFirst {
				// ICU's upper first: the case bits flipped before the tertiary
				c := uint64(e.c) ^ 3
				if e.p == 0 && e.s == 0 {
					c = uint64(e.c) + 1
				}
				w |= c << 32
			}
			out = append(out, w)
		}
	}
	if n == 2 && t.backwards {
		for i, j := 0, len(out)-1; i < j; i, j = i+1, j-1 {
			out[i], out[j] = out[j], out[i]
		}
	}
	return out
}

func (c collator) compare(a, b string) int {
	x, y := c.elements(a), c.elements(b)
	for n := 1; n <= 3; n++ {
		l, r := c.level(x, n), c.level(y, n)
		for i := 0; i < len(l) && i < len(r); i++ {
			if l[i] != r[i] {
				if l[i] < r[i] {
					return -1
				}
				return 1
			}
		}
		if len(l) != len(r) {
			if len(l) < len(r) {
				return -1
			}
			return 1
		}
	}
	return 0
}

// LocaleCompare is `self` against `that` as JS `localeCompare` orders them.
func LocaleCompare(locale, that, self Value) Value {
	return ordering(collatorOf(parseLocale(locale.(string))).compare(self.(string), that.(string)))
}
