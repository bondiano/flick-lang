package flickrt

import (
	"fmt"
	"strings"
)

// The exact-length analysis of a pattern's nodes: which lengths each
// matches up to a limit, and a value of a chosen length, random or
// canonical.

func concatLengths(left, right []bool, limit int) []bool {
	out := make([]bool, limit+1)
	for l := 0; l <= limit && l < len(left); l++ {
		if !left[l] {
			continue
		}
		for r := 0; r+l <= limit && r < len(right); r++ {
			if right[r] {
				out[l+r] = true
			}
		}
	}
	return out
}

// repeatBounds are the counts of a repetition that can matter at `limit`.
func repeatBounds(n *reNode, child []bool, limit int) (int, int) {
	smallest := 1
	for smallest <= limit && !child[smallest] {
		smallest++
	}
	relevant := 0
	if smallest <= limit {
		relevant = limit / smallest
	}
	minimum, maximum := n.minimum, relevant
	if child[0] {
		minimum = 0
	}
	if n.maximum >= 0 {
		maximum = min(n.maximum, relevant)
	}
	return minimum, maximum
}

func (g *regexpGen) lengths(n *reNode, limit int) []bool {
	if cached, ok := g.cache[n]; ok && len(cached) > limit {
		return cached
	}
	out := make([]bool, limit+1)
	switch n.tag {
	case reLiteral:
		if len(n.value) <= limit {
			out[len(n.value)] = true
		}
	case reCharacter:
		for l := 1; l <= 2 && l <= limit; l++ {
			for _, x := range n.intervals {
				if countInInterval(x, l) > 0 {
					out[l] = true
				}
			}
		}
	case reAlt:
		for _, c := range n.nodes {
			for l, ok := range g.lengths(c, limit)[:limit+1] {
				out[l] = out[l] || ok
			}
		}
	case reConcat:
		out[0] = true
		for _, c := range n.nodes {
			out = concatLengths(out, g.lengths(c, limit), limit)
		}
	case reRepeat:
		child := g.lengths(n.node, limit)
		minimum, maximum := repeatBounds(n, child, limit)
		if minimum <= maximum {
			current := make([]bool, limit+1)
			current[0] = true
			for count := 0; count <= maximum; count++ {
				if count >= minimum {
					for l := 0; l <= limit; l++ {
						out[l] = out[l] || current[l]
					}
				}
				if count < maximum {
					current = concatLengths(current, child, limit)
				}
			}
		}
	}
	g.cache[n] = out
	return out
}

func (g *regexpGen) suffixLengths(nodes []*reNode, length int) [][]bool {
	out := make([][]bool, len(nodes)+1)
	terminal := make([]bool, length+1)
	terminal[0] = true
	out[len(nodes)] = terminal
	for i := len(nodes) - 1; i >= 0; i-- {
		out[i] = concatLengths(g.lengths(nodes[i], length), out[i+1], length)
	}
	return out
}

// sequence is the nodes' values in turn, of `length` units in all: each a
// random length the rest can complete, or the shortest one.
func (g *regexpGen) sequence(nodes []*reNode, length int, st *genState) ([]uint16, bool) {
	suffix := g.suffixLengths(nodes, length)
	remaining := length
	var out []uint16
	for i, n := range nodes {
		own, tail := g.lengths(n, remaining), suffix[i+1]
		var candidates []int
		for l := 0; l <= remaining; l++ {
			if own[l] && tail[remaining-l] {
				candidates = append(candidates, l)
				if st == nil {
					break
				}
			}
		}
		if len(candidates) == 0 {
			return nil, false
		}
		l := candidates[0]
		if st != nil {
			l = candidates[st.index(len(candidates))]
		}
		v, ok := g.exact(n, l, st)
		if !ok {
			return nil, false
		}
		out = append(out, v...)
		remaining -= l
	}
	return out, remaining == 0
}

func (g *regexpGen) repeatCounts(n *reNode, length int) []int {
	child := g.lengths(n.node, length)
	minimum, maximum := repeatBounds(n, child, length)
	var counts []int
	if minimum > maximum {
		return counts
	}
	current := make([]bool, length+1)
	current[0] = true
	for count := 0; count <= maximum; count++ {
		if count >= minimum && current[length] {
			counts = append(counts, count)
		}
		if count < maximum {
			current = concatLengths(current, child, length)
		}
	}
	return counts
}

// exact is a value of the node of `length` units; with no state, the
// canonical one.
func (g *regexpGen) exact(n *reNode, length int, st *genState) ([]uint16, bool) {
	switch n.tag {
	case reLiteral:
		return n.value, len(n.value) == length
	case reCharacter:
		if st == nil {
			return canonicalCharacter(n, length)
		}
		return generateCharacter(n, length, st)
	case reAlt:
		var candidates []*reNode
		for _, c := range n.nodes {
			if g.lengths(c, length)[length] {
				candidates = append(candidates, c)
				if st == nil {
					break
				}
			}
		}
		if len(candidates) == 0 {
			return nil, false
		}
		if st == nil {
			return g.exact(candidates[0], length, nil)
		}
		return g.exact(candidates[st.index(len(candidates))], length, st)
	case reConcat:
		return g.sequence(n.nodes, length, st)
	}
	counts := g.repeatCounts(n, length)
	if len(counts) == 0 {
		return nil, false
	}
	count := counts[0]
	if st != nil {
		count = counts[st.index(len(counts))]
	}
	nodes := make([]*reNode, count)
	for i := range nodes {
		nodes[i] = n.node
	}
	return g.sequence(nodes, length, st)
}

func (g *regexpGen) canonical(n *reNode, length int) ([]uint16, bool) { return g.exact(n, length, nil) }

// jsWhitespace is JS's `\s` as the members of a Go class.
const jsWhitespace = `\t-\r \x{a0}\x{1680}\x{2000}-\x{200a}\x{2028}\x{2029}\x{202f}\x{205f}\x{3000}\x{feff}`

// goPattern is a JS pattern without flags in Go's syntax, for the
// generator's final test of a value.
//
// shortcut: lookarounds, backreferences and `{` as a literal have no Go form, so a value of such a pattern is not tested (the generator does not support most of them either)
func goPattern(src []uint16) string {
	var b strings.Builder
	inClass := false
	for i := 0; i < len(src); i++ {
		u := src[i]
		switch {
		case u == '\\' && i+1 < len(src):
			i++
			switch e := src[i]; e {
			case 's':
				if inClass {
					b.WriteString(jsWhitespace)
				} else {
					b.WriteString("[" + jsWhitespace + "]")
				}
			case 'S':
				b.WriteString("[^" + jsWhitespace + "]")
			case 'v':
				b.WriteString(`\x0B`)
			case '0':
				b.WriteString(`\x00`)
			case 'c':
				if i+1 < len(src) {
					i++
					fmt.Fprintf(&b, `\x{%x}`, (src[i]&^0x20)%32)
				}
			case 'u':
				if i+1 < len(src) && src[i+1] == '{' {
					b.WriteString(`\x`)
				} else if i+4 < len(src) {
					fmt.Fprintf(&b, `\x{%s}`, string(utf16Key8(src[i+1:i+5])))
					i += 4
				}
			case '/':
				b.WriteString("/")
			default:
				b.WriteString(`\` + fromUnits([]uint16{e}))
			}
		case u == '[' && !inClass:
			inClass = true
			b.WriteByte('[')
		case u == ']' && inClass:
			inClass = false
			b.WriteByte(']')
		case u == '.' && !inClass:
			b.WriteString(`[^\n\r\x{2028}\x{2029}]`)
		case u == '(' && i+2 < len(src) && src[i+1] == '?' && src[i+2] == '<':
			b.WriteString("(?P<")
			i += 2
		default:
			b.WriteString(fromUnits([]uint16{u}))
		}
	}
	return b.String()
}
