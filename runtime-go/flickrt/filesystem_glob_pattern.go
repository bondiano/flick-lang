package flickrt

import (
	"slices"
	"strings"
)

// Effect's internal/glob.ts compilation: brace expansion, then every
// expanded pattern's segment nodes laid out in one array, so a match state
// is an index into `nodes`.

type fsGlobNodeKind uint8

const (
	fsGlobGlobstar fsGlobNodeKind = iota
	fsGlobLiteral
	fsGlobWildcard
	fsGlobEnd
)

type fsGlobNode struct {
	kind fsGlobNodeKind
	// value is a Literal's name
	value      string
	tokens     []fsGlobToken
	leadingDot bool
	// affixes is set when the segment is `prefix*suffix`
	affixes        bool
	prefix, suffix string
	// directoryOnly is an End that accepts only directories
	directoryOnly bool
}

type fsGlobCompiled struct {
	nodes   []fsGlobNode
	initial []int
	dot     bool
}

const (
	fsGlobMaxExpansions     = 10_000
	fsGlobMaxExpandedLength = 1 << 22
	fsGlobMaxBraceDepth     = 64
)

func fsGlobCompile(patterns []string, dot bool) *fsGlobCompiled {
	compiled := &fsGlobCompiled{dot: dot}
	starts := []int{}
	for _, pattern := range patterns {
		for _, expanded := range fsGlobExpandBraces(pattern) {
			start := len(compiled.nodes)
			if compiled.compilePattern(expanded) {
				starts = append(starts, start)
			}
		}
	}
	compiled.initial = compiled.closure(starts)
	return compiled
}

// fsGlobUnc rejects UNC paths rather than collapsing them into local
// absolute paths.
func fsGlobUnc(path string) bool {
	return strings.HasPrefix(path, "//") && len(path) > 2 && path[2] != '/'
}

func fsGlobDrive(s string) bool {
	return len(s) >= 2 && s[1] == ':' && (s[0] >= 'a' && s[0] <= 'z' || s[0] >= 'A' && s[0] <= 'Z')
}

// fsGlobRoot is the root of an absolute path: `""` or a drive such as `C:/`.
func fsGlobRoot(name string) bool {
	return name == "" || len(name) == 3 && name[2] == '/' && fsGlobDrive(name)
}

func fsGlobAbsolute(path string) bool {
	return strings.HasPrefix(path, "/") || len(path) > 2 && path[2] == '/' && fsGlobDrive(path)
}

// compilePattern appends the pattern's nodes, or nothing when it can match
// no path.
func (c *fsGlobCompiled) compilePattern(pattern string) bool {
	if pattern == "" || fsGlobUnc(pattern) {
		return false
	}
	raw := fsGlobSplitSegments(pattern)
	if globstar := slices.Index(raw, "**"); globstar != -1 {
		for _, segment := range raw[globstar+1:] {
			if segment == "." || segment == ".." {
				return false
			}
		}
	}
	segments, directory := fsGlobNormalize(raw)
	if len(segments) == 0 {
		return false
	}
	for _, segment := range segments {
		if segment != "**" {
			c.nodes = append(c.nodes, fsGlobCompileSegment(fsGlobChars(segment)))
		} else if n := len(c.nodes); n == 0 || c.nodes[n-1].kind != fsGlobGlobstar {
			c.nodes = append(c.nodes, fsGlobNode{kind: fsGlobGlobstar})
		}
	}
	c.nodes = append(c.nodes, fsGlobNode{kind: fsGlobEnd, directoryOnly: directory})
	return true
}

// fsGlobChars is JS `Array.from`: the code points, lone surrogates included.
func fsGlobChars(s string) []rune {
	out := make([]rune, 0, len(s))
	for len(s) > 0 {
		r, n := decode(s)
		out = append(out, r)
		s = s[n:]
	}
	return out
}

func fsGlobSplitSegments(pattern string) []string {
	segments := []string{}
	start := 0
	for i := 0; i < len(pattern); i++ {
		if pattern[i] == '\\' {
			i++
		} else if pattern[i] == '/' {
			segments = append(segments, pattern[start:i])
			start = i + 1
		}
	}
	return append(segments, pattern[start:])
}

// fsGlobNormalize keeps absolute roots and a lone `.`; a trailing `/` or
// `/.` marks a directory.
func fsGlobNormalize(raw []string) ([]string, bool) {
	segments := []string{}
	i := 0
	if len(raw) > 1 && (raw[0] == "" || len(raw[0]) == 2 && fsGlobDrive(raw[0])) {
		if raw[0] == "" {
			segments = append(segments, "")
		} else {
			segments = append(segments, raw[0]+"/")
		}
		i = 1
	}
	directory := false
	for ; i < len(raw); i++ {
		if raw[i] == "" || raw[i] == "." {
			directory = i == len(raw)-1 && len(segments) > 0
		} else {
			segments = append(segments, raw[i])
			directory = false
		}
	}
	if len(segments) == 0 && slices.Contains(raw, ".") {
		segments = append(segments, ".")
	}
	return segments, directory
}

func fsGlobCompileSegment(chars []rune) fsGlobNode {
	tokens := []fsGlobToken{}
	literal := []byte{}
	magic := false
	// once a class fails to close, every later `[` in the segment is literal
	classes := true
	for i := 0; i < len(chars); i++ {
		char := chars[i]
		switch {
		case char == '\\' && i+1 < len(chars):
			i++
			literal = encodeRune(literal, chars[i])
			tokens = append(tokens, fsGlobToken{kind: fsGlobChar, code: chars[i]})
		case char == '*':
			magic = true
			if len(tokens) == 0 || tokens[len(tokens)-1].kind != fsGlobStar {
				tokens = append(tokens, fsGlobToken{kind: fsGlobStar})
			}
		case char == '?':
			magic = true
			tokens = append(tokens, fsGlobToken{kind: fsGlobAny})
		case char == '[' && classes:
			if token, end, ok := fsGlobParseClass(chars, i); ok {
				magic = true
				tokens = append(tokens, token)
				i = end
			} else {
				classes = false
				literal = append(literal, '[')
				tokens = append(tokens, fsGlobToken{kind: fsGlobChar, code: '['})
			}
		default:
			literal = encodeRune(literal, char)
			tokens = append(tokens, fsGlobToken{kind: fsGlobChar, code: char})
		}
	}
	if !magic {
		return fsGlobNode{kind: fsGlobLiteral, value: string(literal)}
	}
	node := fsGlobNode{kind: fsGlobWildcard, tokens: tokens, leadingDot: tokens[0].kind == fsGlobChar && tokens[0].code == '.'}
	node.prefix, node.suffix, node.affixes = fsGlobAffixes(tokens)
	return node
}

// fsGlobAffixes splits `prefix*suffix`; lone surrogates stay on the token
// path, as a prefix test would match them against half of a pair.
func fsGlobAffixes(tokens []fsGlobToken) (string, string, bool) {
	var prefix, suffix []byte
	star := false
	for _, token := range tokens {
		switch {
		case token.kind == fsGlobStar && !star:
			star = true
		case token.kind == fsGlobChar && (token.code < 0xd800 || token.code > 0xdfff):
			if star {
				suffix = encodeRune(suffix, token.code)
			} else {
				prefix = encodeRune(prefix, token.code)
			}
		default:
			return "", "", false
		}
	}
	return string(prefix), string(suffix), star
}

// fsGlobMatchWildcard backtracks only to the last `*`.
func fsGlobMatchWildcard(node *fsGlobNode, name string, dot bool) bool {
	if name == "." || name == ".." || strings.HasPrefix(name, ".") && !dot && !node.leadingDot {
		return false
	}
	if node.affixes {
		return len(name) >= len(node.prefix)+len(node.suffix) && strings.HasPrefix(name, node.prefix) && strings.HasSuffix(name, node.suffix)
	}
	tokens := node.tokens
	t, c, star, resume := 0, 0, -1, 0
	for c < len(name) {
		if t < len(tokens) && tokens[t].kind == fsGlobStar {
			star, resume = t, c
			t++
			continue
		}
		code, width := decode(name[c:])
		if t < len(tokens) && tokens[t].match(code) {
			t++
			c += width
		} else if star != -1 {
			t = star + 1
			_, width := decode(name[resume:])
			resume += width
			c = resume
		} else {
			return false
		}
	}
	for t < len(tokens) && tokens[t].kind == fsGlobStar {
		t++
	}
	return t == len(tokens)
}

// closure adds the successor of each `**`, which also matches zero
// segments; states come back sorted and unique.
func (c *fsGlobCompiled) closure(states []int) []int {
	for i := 0; i < len(states); i++ {
		if c.nodes[states[i]].kind == fsGlobGlobstar {
			states = append(states, states[i]+1)
		}
	}
	slices.Sort(states)
	return slices.Compact(states)
}

// step is the states reached by matching a literal or wildcard segment and
// those reached by a `**` consuming `name`.
func (c *fsGlobCompiled) step(states []int, name string) (explicit, globstar []int) {
	// only a literal segment can name the root of an absolute path
	root := fsGlobRoot(name)
	for _, state := range states {
		node := &c.nodes[state]
		switch node.kind {
		case fsGlobGlobstar:
			if !root && name != "." && name != ".." && (c.dot || !strings.HasPrefix(name, ".")) {
				globstar = append(globstar, state)
			}
		case fsGlobLiteral:
			if node.value == name {
				explicit = append(explicit, state+1)
			}
		case fsGlobWildcard:
			if !root && fsGlobMatchWildcard(node, name, c.dot) {
				explicit = append(explicit, state+1)
			}
		}
	}
	return c.closure(explicit), c.closure(globstar)
}

func fsGlobUnion(explicit, globstar []int) []int {
	if len(globstar) == 0 {
		return explicit
	}
	states := slices.Concat(explicit, globstar)
	slices.Sort(states)
	return slices.Compact(states)
}

func (c *fsGlobCompiled) accepts(states []int, directory bool) bool {
	for _, state := range states {
		if node := c.nodes[state]; node.kind == fsGlobEnd && (directory || !node.directoryOnly) {
			return true
		}
	}
	return false
}

func (c *fsGlobCompiled) canDescend(states []int) bool {
	for _, state := range states {
		if c.nodes[state].kind != fsGlobEnd {
			return true
		}
	}
	return false
}

func (c *fsGlobCompiled) needsDirectory(states []int) bool {
	for _, state := range states {
		if node := c.nodes[state]; node.kind == fsGlobEnd && node.directoryOnly {
			return true
		}
	}
	return false
}
