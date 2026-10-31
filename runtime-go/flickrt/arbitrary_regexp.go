package flickrt

import (
	"math"
	"regexp"
	"slices"
	"strconv"
)

// Strings that match a pattern (internal/arbitrary/regexp.ts): the
// pattern parsed into literals, character classes, sequences, choices and
// repetitions, each generated at an exact length in UTF-16 code units.

type reTag int

const (
	reLiteral reTag = iota
	reCharacter
	reConcat
	reAlt
	reRepeat
)

type interval struct{ lo, hi int }

type reNode struct {
	tag       reTag
	value     []uint16 // literal
	intervals []interval
	preferred [2][]int // character: preferred code points of 1 and 2 units
	counts    [2]int
	nodes     []*reNode // concatenation, alternation
	node      *reNode   // repetition
	minimum   int
	maximum   int // -1: unbounded
}

var (
	emptyLiteral       = &reNode{tag: reLiteral}
	digitIntervals     = []interval{{48, 57}}
	wordIntervals      = []interval{{48, 57}, {65, 90}, {95, 95}, {97, 122}}
	whitespaceInterval = []interval{{9, 13}, {32, 32}, {160, 160}, {0x1680, 0x1680}, {0x2000, 0x200a},
		{0x2028, 0x2029}, {0x202f, 0x202f}, {0x205f, 0x205f}, {0x3000, 0x3000}, {0xfeff, 0xfeff}}
	lineTerminators = []interval{{10, 10}, {13, 13}, {0x2028, 0x2029}}
	preferredPoints = func() []int {
		out := make([]int, 0, 104)
		for i := 32; i < 127; i++ {
			out = append(out, i)
		}
		return append(out, 9, 10, 11, 12, 13, 0, 0x80, 0xe9, 0x3a9, 0x1f600)
	}()
)

func pointLength(cp int) int {
	if cp > 0xffff {
		return 2
	}
	return 1
}

func inIntervals(cp int, xs []interval) bool {
	for _, x := range xs {
		if cp < x.lo {
			return false
		}
		if cp <= x.hi {
			return true
		}
	}
	return false
}

func countInInterval(x interval, length int) int {
	lo, hi := x.lo, min(x.hi, 0xffff)
	if length != 1 {
		lo, hi = max(x.lo, 0x10000), x.hi
	}
	if lo > hi {
		return 0
	}
	if length == 1 && lo <= 0xdfff && hi >= 0xd800 {
		return hi - lo + 1 - (min(hi, 0xdfff) - max(lo, 0xd800) + 1)
	}
	return hi - lo + 1
}

func pointAt(x interval, length, offset int) int {
	cp := x.lo + offset
	if length != 1 {
		cp = max(x.lo, 0x10000) + offset
	}
	if length == 1 && cp >= 0xd800 {
		cp += 0x800
	}
	return cp
}

func characterNode(xs []interval) *reNode {
	xs = normalizeIntervals(xs)
	n := &reNode{tag: reCharacter, intervals: xs}
	for l := 1; l <= 2; l++ {
		for _, cp := range preferredPoints {
			if pointLength(cp) == l && inIntervals(cp, xs) {
				n.preferred[l-1] = append(n.preferred[l-1], cp)
			}
		}
		for _, x := range xs {
			n.counts[l-1] += countInInterval(x, l)
		}
	}
	return n
}

func literalRe(v []uint16) *reNode {
	if len(v) == 0 {
		return emptyLiteral
	}
	return &reNode{tag: reLiteral, value: v}
}

func concatRe(nodes []*reNode) *reNode {
	var flat []*reNode
	var pending []uint16
	flush := func() {
		if len(pending) > 0 {
			flat = append(flat, literalRe(pending))
			pending = nil
		}
	}
	for _, n := range nodes {
		switch n.tag {
		case reLiteral:
			pending = append(pending, n.value...)
		case reConcat:
			flush()
			flat = append(flat, n.nodes...)
		default:
			flush()
			flat = append(flat, n)
		}
	}
	flush()
	switch len(flat) {
	case 0:
		return emptyLiteral
	case 1:
		return flat[0]
	}
	return &reNode{tag: reConcat, nodes: flat}
}

func altRe(nodes []*reNode) *reNode {
	var flat []*reNode
	for _, n := range nodes {
		if n.tag == reAlt {
			flat = append(flat, n.nodes...)
		} else {
			flat = append(flat, n)
		}
	}
	if len(flat) == 1 {
		return flat[0]
	}
	return &reNode{tag: reAlt, nodes: flat}
}

func normalizeIntervals(xs []interval) []interval {
	if len(xs) <= 1 {
		return xs
	}
	sorted := slices.Clone(xs)
	slices.SortFunc(sorted, func(a, b interval) int {
		if a.lo != b.lo {
			return a.lo - b.lo
		}
		return a.hi - b.hi
	})
	var out []interval
	for _, x := range sorted {
		if len(out) == 0 || x.lo > out[len(out)-1].hi+1 {
			out = append(out, x)
		} else if x.hi > out[len(out)-1].hi {
			out[len(out)-1].hi = x.hi
		}
	}
	return out
}

func restrictPoints(n *reNode, maximum int) *reNode {
	switch n.tag {
	case reCharacter:
		var xs []interval
		for _, x := range n.intervals {
			if x.lo <= maximum {
				xs = append(xs, interval{x.lo, min(x.hi, maximum)})
			}
		}
		return characterNode(xs)
	case reConcat, reAlt:
		nodes := make([]*reNode, len(n.nodes))
		for i, c := range n.nodes {
			nodes[i] = restrictPoints(c, maximum)
		}
		if n.tag == reConcat {
			return concatRe(nodes)
		}
		return altRe(nodes)
	case reRepeat:
		c := *n
		c.node = restrictPoints(n.node, maximum)
		return &c
	}
	return n
}

func complementIntervals(xs []interval) []interval {
	var out []interval
	lo := 0
	for _, x := range normalizeIntervals(xs) {
		if lo < x.lo {
			out = append(out, interval{lo, x.lo - 1})
		}
		lo = max(lo, x.hi+1)
	}
	if lo <= 0x10ffff {
		out = append(out, interval{lo, 0x10ffff})
	}
	return out
}

func pointUnits(cp int) []uint16 {
	if cp > 0xffff {
		r := cp - 0x10000
		return []uint16{uint16(0xd800 + r>>10), uint16(0xdc00 + r&0x3ff)}
	}
	return []uint16{uint16(cp)}
}

func generateCharacter(n *reNode, length int, st *genState) ([]uint16, bool) {
	preferred := n.preferred[length-1]
	if len(preferred) > 0 && st.biased() {
		return pointUnits(preferred[st.index(len(preferred))]), true
	}
	count := n.counts[length-1]
	if count == 0 {
		return nil, false
	}
	offset := int(st.randomInt(0, float64(count-1)))
	for _, x := range n.intervals {
		size := countInInterval(x, length)
		if offset < size {
			return pointUnits(pointAt(x, length, offset)), true
		}
		offset -= size
	}
	return nil, false
}

func canonicalCharacter(n *reNode, length int) ([]uint16, bool) {
	if p := n.preferred[length-1]; len(p) > 0 {
		return pointUnits(p[0]), true
	}
	for _, x := range n.intervals {
		if countInInterval(x, length) > 0 {
			return pointUnits(pointAt(x, length, 0)), true
		}
	}
	return nil, false
}

func isDigitUnit(u int) bool { return u >= '0' && u <= '9' }

func isHexUnit(u int) bool {
	return isDigitUnit(u) || u >= 'a' && u <= 'f' || u >= 'A' && u <= 'F'
}

// reParser is Effect's parser of the supported subset; anything else
// makes the pattern unsupported, and the string generator ignores it.
type reParser struct {
	src       []uint16
	i         int
	supported bool
	dotAll    bool
}

func (p *reParser) peek() int {
	if p.i < len(p.src) {
		return int(p.src[p.i])
	}
	return -1
}

func (p *reParser) next() int { u := p.peek(); p.i++; return u }

func (p *reParser) parse() *reNode {
	n := p.alternation()
	if p.supported && p.i == len(p.src) {
		return n
	}
	return nil
}

func (p *reParser) alternation() *reNode {
	nodes := []*reNode{p.concatenation()}
	for p.peek() == '|' {
		p.i++
		nodes = append(nodes, p.concatenation())
	}
	return altRe(nodes)
}

func (p *reParser) concatenation() *reNode {
	var nodes []*reNode
	for p.i < len(p.src) && p.peek() != ')' && p.peek() != '|' {
		atom := p.atom()
		if atom == nil {
			p.supported = false
			break
		}
		nodes = append(nodes, p.quantifier(atom))
	}
	return concatRe(nodes)
}

func (p *reParser) atom() *reNode {
	switch t := p.next(); t {
	case '^', '$':
		return emptyLiteral
	case '.':
		if p.dotAll {
			return characterNode([]interval{{0, 0x10ffff}})
		}
		return characterNode(complementIntervals(lineTerminators))
	case '[':
		return p.class()
	case '(':
		return p.group()
	case '\\':
		return p.escape(false)
	case '*', '+', '?', '{', ')', -1:
		return nil
	default:
		return literalRe([]uint16{uint16(t)})
	}
}

func (p *reParser) group() *reNode {
	if p.peek() == '?' {
		p.i++
		switch p.peek() {
		case ':':
			p.i++
		case '<':
			if p.i+1 < len(p.src) && (p.src[p.i+1] == '=' || p.src[p.i+1] == '!') {
				return nil
			}
			p.i++
			for p.i < len(p.src) && p.peek() != '>' {
				p.i++
			}
			if p.peek() != '>' {
				return nil
			}
			p.i++
		default:
			return nil
		}
	}
	n := p.alternation()
	if p.peek() != ')' {
		return nil
	}
	p.i++
	return n
}

func (p *reParser) quantifier(n *reNode) *reNode {
	start := p.i
	minimum, maximum := 0, -1
	switch p.peek() {
	case '*':
		p.i++
	case '+':
		minimum = 1
		p.i++
	case '?':
		maximum = 1
		p.i++
	case '{':
		p.i++
		from, ok := p.natural()
		if !ok {
			p.i = start
			return n
		}
		minimum = from
		switch p.peek() {
		case '}':
			maximum = from
			p.i++
		case ',':
			p.i++
			if to, ok := p.natural(); ok {
				maximum = to
			}
			if p.peek() != '}' {
				p.i = start
				return n
			}
			p.i++
		default:
			p.i = start
			return n
		}
	default:
		return n
	}
	if p.peek() == '?' {
		p.i++
	}
	return &reNode{tag: reRepeat, node: n, minimum: minimum, maximum: maximum}
}

func (p *reParser) natural() (int, bool) {
	start := p.i
	for isDigitUnit(p.peek()) {
		p.i++
	}
	if start == p.i {
		return 0, false
	}
	v, err := strconv.ParseFloat(string(utf16Key8(p.src[start:p.i])), 64)
	if err != nil || v > maxSafe {
		return 0, false
	}
	return int(v), true
}

// utf16Key8 is ASCII units as bytes.
func utf16Key8(units []uint16) []byte {
	out := make([]byte, len(units))
	for i, u := range units {
		out[i] = byte(u)
	}
	return out
}

func (p *reParser) class() *reNode {
	negative := p.peek() == '^'
	if negative {
		p.i++
	}
	var xs []interval
	for first := true; p.i < len(p.src) && (p.peek() != ']' || first); first = false {
		left := p.classAtom()
		if left == nil {
			return nil
		}
		if p.peek() == '-' && p.i+1 < len(p.src) && p.src[p.i+1] != ']' {
			p.i++
			right := p.classAtom()
			if right == nil || len(left) != 1 || len(right) != 1 || left[0].lo != left[0].hi ||
				right[0].lo != right[0].hi || left[0].lo > right[0].lo {
				return nil
			}
			xs = append(xs, interval{left[0].lo, right[0].lo})
		} else {
			xs = append(xs, left...)
		}
	}
	if p.peek() != ']' {
		return nil
	}
	p.i++
	xs = normalizeIntervals(xs)
	if negative {
		xs = complementIntervals(xs)
	}
	return characterNode(xs)
}

func (p *reParser) classAtom() []interval {
	t := p.next()
	switch t {
	case '\\':
		e := p.escape(true)
		switch {
		case e == nil:
			return nil
		case e.tag == reCharacter:
			return e.intervals
		}
		switch v := e.value; {
		case len(v) == 1:
			return []interval{{int(v[0]), int(v[0])}}
		case len(v) == 2 && v[0] >= 0xd800 && v[0] <= 0xdbff && v[1] >= 0xdc00 && v[1] <= 0xdfff:
			cp := 0x10000 + (int(v[0])-0xd800)<<10 + int(v[1]) - 0xdc00
			return []interval{{cp, cp}}
		}
		return nil
	case -1:
		return nil
	}
	return []interval{{t, t}}
}

func (p *reParser) escape(inClass bool) *reNode {
	t := p.next()
	switch t {
	case 'd':
		return characterNode(digitIntervals)
	case 'D':
		return characterNode(complementIntervals(digitIntervals))
	case 'w':
		return characterNode(wordIntervals)
	case 'W':
		return characterNode(complementIntervals(wordIntervals))
	case 's':
		return characterNode(whitespaceInterval)
	case 'S':
		return characterNode(complementIntervals(whitespaceInterval))
	case 'b':
		if inClass {
			return literalRe([]uint16{8})
		}
		return nil
	case 'B', 'p', 'P', 'k', -1:
		return nil
	case 'n':
		return literalRe([]uint16{'\n'})
	case 'r':
		return literalRe([]uint16{'\r'})
	case 't':
		return literalRe([]uint16{'\t'})
	case 'v':
		return literalRe([]uint16{'\v'})
	case 'f':
		return literalRe([]uint16{'\f'})
	case '0':
		if isDigitUnit(p.peek()) {
			return nil
		}
		return literalRe([]uint16{0})
	case 'c':
		c := p.next()
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z') {
			return nil
		}
		if c >= 'a' {
			c -= 32
		}
		return literalRe([]uint16{uint16(c % 32)})
	case 'x':
		return p.codePoint(2)
	case 'u':
		if p.peek() == '{' {
			p.i++
			start := p.i
			for isHexUnit(p.peek()) {
				p.i++
			}
			if start == p.i || p.peek() != '}' {
				return nil
			}
			v, _ := strconv.ParseInt(string(utf16Key8(p.src[start:p.i])), 16, 64)
			p.i++
			if v > 0x10ffff || v >= 0xd800 && v <= 0xdfff {
				return nil
			}
			return literalRe(pointUnits(int(v)))
		}
		return p.codePoint(4)
	}
	if isDigitUnit(t) {
		return nil
	}
	return literalRe([]uint16{uint16(t)})
}

func (p *reParser) codePoint(length int) *reNode {
	if p.i+length > len(p.src) {
		return nil
	}
	for _, u := range p.src[p.i : p.i+length] {
		if !isHexUnit(int(u)) {
			return nil
		}
	}
	v, _ := strconv.ParseInt(string(utf16Key8(p.src[p.i:p.i+length])), 16, 64)
	p.i += length
	return literalRe([]uint16{uint16(v)})
}

// minimumLength is the fewest units a node matches.
func (n *reNode) minimumLength() float64 {
	switch n.tag {
	case reLiteral:
		return float64(len(n.value))
	case reCharacter:
		switch {
		case len(n.intervals) == 0:
			return math.Inf(1)
		case n.intervals[0].lo > 0xffff:
			return 2
		}
		return 1
	case reAlt:
		out := math.Inf(1)
		for _, c := range n.nodes {
			out = math.Min(out, c.minimumLength())
		}
		return out
	case reConcat:
		out := 0.0
		for _, c := range n.nodes {
			out += c.minimumLength()
		}
		return out
	}
	return n.node.minimumLength() * float64(n.minimum)
}

func hasTrailingAnchor(src []uint16) bool {
	if len(src) == 0 || src[len(src)-1] != '$' {
		return false
	}
	backslashes := 0
	for i := len(src) - 2; i >= 0 && src[i] == '\\'; i-- {
		backslashes++
	}
	return backslashes%2 == 0
}

// regexpGen is a compiled pattern: Effect's `Regexp.Compiled`.
type regexpGen struct {
	node          *reNode
	minimumLength float64
	re            *regexp.Regexp // nil when the go target cannot run it
	cache         map[*reNode][]bool
}

// compilePattern compiles a pattern without flags, as a check of the
// program states one; nil when Effect does not support it.
func compilePattern(source string) *regexpGen {
	src := utf16Units(source)
	parsed := (&reParser{src: src, supported: true}).parse()
	if parsed == nil {
		return nil
	}
	parsed = restrictPoints(parsed, 0xffff)
	anything := &reNode{tag: reRepeat, node: characterNode([]interval{{32, 126}}), maximum: -1}
	nodes := []*reNode{}
	if len(src) == 0 || src[0] != '^' {
		nodes = append(nodes, anything)
	}
	nodes = append(nodes, parsed)
	if !hasTrailingAnchor(src) {
		nodes = append(nodes, anything)
	}
	node := concatRe(nodes)
	re, _ := regexp.Compile(goPattern(src))
	return &regexpGen{node, node.minimumLength(), re, map[*reNode][]bool{}}
}

func (g *regexpGen) test(units []uint16) bool {
	return g.re == nil || g.re.MatchString(fromUnits(units))
}

func (g *regexpGen) generate(st *genState, minimum, maximum float64) ([]uint16, bool) {
	limit := int(math.Min(maximum, 1<<20))
	lengths := g.lengths(g.node, limit)
	var candidates []int
	for l := int(minimum); l <= limit; l++ {
		if lengths[l] {
			candidates = append(candidates, l)
		}
	}
	if len(candidates) == 0 {
		return nil, false
	}
	offset := int(st.randomLength(0, float64(len(candidates)-1)))
	v, ok := g.exact(g.node, candidates[offset], st)
	return v, ok && g.test(v)
}

func (g *regexpGen) shrink(value []uint16, minimum float64) [][]uint16 {
	lengths := g.lengths(g.node, len(value))
	var smaller []int
	for l := int(minimum); l < len(value); l++ {
		if lengths[l] {
			smaller = append(smaller, l)
		}
	}
	var candidates []int
	if len(smaller) > 0 {
		candidates = append(candidates, smaller[0], smaller[(len(smaller)-1)/2], smaller[len(smaller)-1])
	}
	candidates = append(candidates, len(value))
	var out [][]uint16
	for _, l := range candidates {
		c, ok := g.canonical(g.node, l)
		if ok && !slices.Equal(c, value) && !slices.ContainsFunc(out, func(o []uint16) bool { return slices.Equal(o, c) }) && g.test(c) {
			out = append(out, c)
		}
	}
	return out
}
