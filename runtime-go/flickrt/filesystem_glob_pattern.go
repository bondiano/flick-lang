package flickrt

import (
	"runtime"
	"strings"
	"unicode/utf16"
	"unicode/utf8"
)

type fsGlobToken struct {
	op    byte
	char  rune
	class *fsGlobClass
	alts  [][]fsGlobToken
	tail  []fsGlobToken
	empty bool
}

type fsGlobPart struct {
	literal   string
	tokens    []fsGlobToken
	magic     bool
	unicode   bool
	recursive bool
	fast      byte
	qmarks    int
	extLower  string
	extUnits  int
}

const (
	fsGlobFastStar byte = iota + 1
	fsGlobFastStarExt
	fsGlobFastQmarks
)

func fsGlobCompilePart(pattern string) fsGlobPart {
	if pattern == "**" {
		return fsGlobPart{recursive: true, magic: true}
	}
	fast, qmarks, ext := fsGlobFastShape(pattern)
	tokens, magic := fsGlobParse(pattern)
	fsGlobFlatten(&tokens)
	unicodeMode := fsGlobUnicode(tokens)
	if !unicodeMode {
		if encoded := fsGlobUTF16(pattern); encoded != pattern {
			tokens, magic = fsGlobParse(encoded)
			fsGlobFlatten(&tokens)
		}
	}
	var literal strings.Builder
	if !magic {
		for i := 0; i < len(tokens); i++ {
			ch := tokens[i].char
			if ch >= 0xd800 && ch <= 0xdbff && i+1 < len(tokens) && tokens[i+1].char >= 0xdc00 && tokens[i+1].char <= 0xdfff {
				ch = utf16.DecodeRune(ch, tokens[i+1].char)
				i++
			}
			literal.WriteRune(ch)
		}
	}
	fsGlobFillNegs(tokens, nil)
	part := fsGlobPart{literal: literal.String(), tokens: tokens, magic: magic, unicode: unicodeMode, fast: fast, qmarks: qmarks}
	if (runtime.GOOS == "windows" || runtime.GOOS == "darwin") && (fast == fsGlobFastStarExt || fast == fsGlobFastQmarks) {
		part.extLower = ToLowerCase(ext).(string)
		if fast == fsGlobFastQmarks {
			part.extUnits = fsGlobUTF16Units(ext)
		}
	}
	return part
}

// fsGlobFastShape is minimatch's parse-time fast-test dispatch: these shapes
// compare lowercased strings rather than using regex `/i` case canonicalization.
func fsGlobFastShape(pattern string) (byte, int, string) {
	stars := 0
	for stars < len(pattern) && pattern[stars] == '*' {
		stars++
	}
	if stars != 0 {
		if stars == len(pattern) {
			return fsGlobFastStar, 0, ""
		}
		if rest := pattern[stars:]; fsGlobExtOnly(rest) {
			return fsGlobFastStarExt, 0, rest
		}
	}
	marks := 0
	for marks < len(pattern) && pattern[marks] == '?' {
		marks++
	}
	if marks != 0 {
		if rest := pattern[marks:]; fsGlobExtOnly(rest) {
			return fsGlobFastQmarks, marks, rest
		}
	}
	return 0, 0, ""
}

func fsGlobExtOnly(text string) bool {
	for _, ch := range text {
		if strings.ContainsRune("+@!?*[(", ch) {
			return false
		}
	}
	return true
}

// fsGlobUTF16Units counts JS string length; surrogates count two code units.
func fsGlobUTF16Units(text string) int {
	units := 0
	for len(text) != 0 {
		ch, size := decode(text)
		text = text[size:]
		if ch > 0xffff {
			units += 2
		} else {
			units++
		}
	}
	return units
}

func fsGlobUnicode(tokens []fsGlobToken) bool {
	for _, token := range tokens {
		if token.class != nil {
			for _, class := range token.class.posix {
				if class != "ascii" && class != "xdigit" {
					return true
				}
			}
		}
		for _, alternative := range token.alts {
			if fsGlobUnicode(alternative) {
				return true
			}
		}
	}
	return false
}

// JS regexes consume UTF-16 code units unless a Unicode POSIX class sets /u.
func fsGlobUTF16(text string) string {
	astral := strings.IndexFunc(text, func(ch rune) bool { return ch > 0xffff })
	if astral < 0 {
		return text
	}
	var out strings.Builder
	out.Grow(len(text) + 2)
	out.WriteString(text[:astral])
	for _, ch := range text[astral:] {
		if ch <= 0xffff {
			out.WriteRune(ch)
			continue
		}
		hi, lo := utf16.EncodeRune(ch)
		for _, unit := range [2]rune{hi, lo} {
			out.WriteByte(byte(0xe0 | unit>>12))
			out.WriteByte(byte(0x80 | unit>>6&0x3f))
			out.WriteByte(byte(0x80 | unit&0x3f))
		}
	}
	return out.String()
}

func fsGlobRune(text string) (rune, int) {
	if len(text) >= 3 && text[0] == 0xed && text[1] >= 0xa0 && text[1] <= 0xbf && text[2]&0xc0 == 0x80 {
		return rune(text[0]&0xf)<<12 | rune(text[1]&0x3f)<<6 | rune(text[2]&0x3f), 3
	}
	return utf8.DecodeRuneInString(text)
}

func fsGlobParse(pattern string) ([]fsGlobToken, bool) {
	// An empty positive extglob as the whole pattern is literal in minimatch.
	if len(pattern) == 3 && pattern[1:] == "()" && strings.ContainsRune("?*+@", rune(pattern[0])) {
		return fsGlobParseLiteral(pattern), false
	}
	tokens, end := fsGlobParseSequence(pattern, 0, 0, 0, 0)
	if end != len(pattern) {
		return fsGlobParseLiteral(pattern), false
	}
	return tokens, fsGlobHasMagic(tokens)
}

func fsGlobParseLiteral(pattern string) []fsGlobToken {
	tokens := []fsGlobToken{}
	for _, ch := range pattern {
		tokens = append(tokens, fsGlobToken{op: '=', char: ch})
	}
	return tokens
}

func fsGlobHasMagic(tokens []fsGlobToken) bool {
	for _, token := range tokens {
		if token.alts != nil || token.op == '*' || token.op == '?' || token.op == '[' {
			return true
		}
	}
	return false
}

// parse a sequence of plain tokens and groups until `)` or the end; returns
// the tokens and the index just past the last one consumed.
func fsGlobParseSequence(pattern string, start, depth, groupDepth int, parent byte) ([]fsGlobToken, int) {
	tokens := []fsGlobToken{}
	for i := start; i < len(pattern); {
		ch, size := fsGlobRune(pattern[i:])
		adopted := fsGlobCanAdopt(parent, byte(ch))
		if strings.ContainsRune("?*+@!", ch) && i+1 < len(pattern) && pattern[i+1] == '(' && (groupDepth <= 2 || adopted) {
			inner := groupDepth
			if !adopted {
				inner++
			}
			group, end, ok := fsGlobParseGroup(pattern, i, inner)
			if ok {
				tokens = append(tokens, group)
				i = end
				continue
			}
		}
		if ch == '[' {
			if token, consumed, _ := fsGlobParseClass(pattern[i:]); consumed != 0 {
				tokens = append(tokens, token)
				i += consumed
				continue
			}
		}
		if ch == ')' && depth > 0 {
			return tokens, i
		}
		if ch == '|' && depth > 0 {
			return tokens, i
		}
		if ch == '*' {
			if len(tokens) == 0 || tokens[len(tokens)-1].op != '*' || tokens[len(tokens)-1].alts != nil {
				tokens = append(tokens, fsGlobToken{op: '*'})
			}
		} else if ch == '?' {
			tokens = append(tokens, fsGlobToken{op: '?'})
		} else {
			tokens = append(tokens, fsGlobToken{op: '=', char: ch})
		}
		i += size
	}
	return tokens, len(pattern)
}

func fsGlobParseGroup(pattern string, start, groupDepth int) (fsGlobToken, int, bool) {
	op := pattern[start]
	group := fsGlobToken{op: op, alts: [][]fsGlobToken{}}
	alternative, end := fsGlobParseSequence(pattern, start+2, 1, groupDepth, op)
	group.alts = append(group.alts, alternative)
	for end < len(pattern) {
		if pattern[end] == '|' {
			next := end + 1
			if next < len(pattern) && pattern[next] == ')' {
				group.alts = append(group.alts, nil)
				group.finishEmpty()
				return group, next + 1, true
			}
			alternative, end = fsGlobParseSequence(pattern, next, 1, groupDepth, op)
			group.alts = append(group.alts, alternative)
			continue
		}
		if pattern[end] == ')' {
			// minimatch demotes an empty positive group with nothing around it.
			if op != '!' && len(group.alts) == 1 && len(group.alts[0]) == 0 && start == 0 && end == len(pattern)-1 {
				return fsGlobToken{}, 0, false
			}
			group.finishEmpty()
			return group, end + 1, true
		}
		// something followed the closing `)` of a nested group: treat the
		// whole construct as a literal.
		return fsGlobToken{}, 0, false
	}
	return fsGlobToken{}, 0, false
}

// finishEmpty is minimatch's `#emptyExt`: a negative group whose content ends
// at a nested group (or at `|`/`)`) closes with `acc == ""`, so its regex uses
// `starNoEmpty` and ignores the body. `#flatten` resets it when this group is
// usurped, which happens exactly when its normalized type is no longer `!`.
func (group *fsGlobToken) finishEmpty() {
	if group.op != '!' {
		return
	}
	last := group.alts[len(group.alts)-1]
	// Syntax only: fsGlobFlatten applies native adopt/usurp and resets it.
	group.empty = len(last) == 0 || last[len(last)-1].alts != nil
}

// fsGlobFlatten applies minimatch's #flatten to a parsed token tree: children
// first, then adopt/adoptWithSpace/usurp on each sole nested group, up to the
// same ten passes. This mutates the actual types and bodies the matcher uses.
func fsGlobFlatten(tokens *[]fsGlobToken) {
	for i := range *tokens {
		fsGlobFlattenToken(&(*tokens)[i])
	}
}

func fsGlobFlattenToken(group *fsGlobToken) {
	if group.alts == nil {
		return
	}
	for j := range group.alts {
		for k := range group.alts[j] {
			fsGlobFlattenToken(&group.alts[j][k])
		}
	}
	for {
		changed := false
		for j := 0; j < len(group.alts); j++ {
			alternative := group.alts[j]
			if len(alternative) != 1 || alternative[0].alts == nil {
				continue
			}
			child := alternative[0]
			if fsGlobAdoptable(group.op, child.op) {
				group.alts = fsGlobSplice(group.alts, j, child.alts)
				changed = true
				break
			}
			if fsGlobAdoptableWithSpace(group.op, child.op) {
				insert := append(append([][]fsGlobToken{}, child.alts...), nil)
				group.alts = fsGlobSplice(group.alts, j, insert)
				changed = true
				break
			}
			if len(group.alts) == 1 {
				if usurped, ok := fsGlobUsurpType(group.op, child.op); ok {
					group.op = usurped
					group.alts = child.alts
					group.empty = false
					changed = true
					break
				}
			}
		}
		if !changed {
			break
		}
	}
}

func fsGlobSplice(alts [][]fsGlobToken, at int, insert [][]fsGlobToken) [][]fsGlobToken {
	out := make([][]fsGlobToken, 0, len(alts)-1+len(insert))
	out = append(out, alts[:at]...)
	out = append(out, insert...)
	return append(out, alts[at+1:]...)
}

// fsGlobAdoptable is minimatch's adoptionMap; fsGlobCanAdopt is adoptionAnyMap.
func fsGlobAdoptable(parent, child byte) bool {
	switch parent {
	case '!':
		return child == '@'
	case '?':
		return child == '?' || child == '@'
	case '@':
		return child == '@'
	case '*':
		return child == '*' || child == '+' || child == '?' || child == '@'
	case '+':
		return child == '+' || child == '@'
	}
	return false
}

// fsGlobAdoptableWithSpace is minimatch's adoptionWithSpaceMap: the adopted
// group also gains an empty alternative.
func fsGlobAdoptableWithSpace(parent, child byte) bool {
	switch parent {
	case '!', '@':
		return child == '?'
	case '+':
		return child == '?' || child == '*'
	}
	return false
}

// fsGlobCanAdopt is minimatch's adoptionAnyMap.
func fsGlobCanAdopt(parent, child byte) bool {
	switch parent {
	case '!', '?', '@':
		return child == '?' || child == '@'
	case '*':
		return child == '*' || child == '+' || child == '?' || child == '@'
	case '+':
		return child == '+' || child == '@' || child == '?' || child == '*'
	}
	return false
}

// fsGlobUsurpType is minimatch's usurpMap.
func fsGlobUsurpType(group, child byte) (byte, bool) {
	switch group {
	case '!':
		if child == '!' {
			return '@', true
		}
	case '?':
		if child == '*' || child == '+' {
			return '*', true
		}
	case '@':
		if strings.ContainsRune("!?@*+", rune(child)) {
			return child, true
		}
	case '+':
		if child == '?' || child == '*' {
			return '*', true
		}
	}
	return 0, false
}

// Minimatch's negative lookahead includes the rest of its enclosing sequences.
// The continuation is built only where a reachable negative consumes it, so a
// pattern of many positive groups does not copy a suffix per group.
func fsGlobFillNegs(tokens, suffix []fsGlobToken) {
	for i := len(tokens) - 1; i >= 0; i-- {
		group := &tokens[i]
		if group.alts == nil {
			continue
		}
		if group.op == '!' {
			if group.empty {
				// an emptyExt negative ignores its body, so no continuation is read
				continue
			}
			rest := fsGlobContinuation(tokens[i+1:], suffix)
			// minimatch's adoption turns a lone trailing `*` of a leading
			// negative group into `starNoEmpty` inside the lookahead.
			group.tail = rest
			if i == 0 && len(rest) == 1 && rest[0].op == '*' && rest[0].alts == nil {
				group.tail = []fsGlobToken{{op: 'N'}}
			}
			for _, alternative := range group.alts {
				fsGlobFillNegs(alternative, rest)
			}
			continue
		}
		if !fsGlobHasNegative(group.alts) {
			continue
		}
		rest := fsGlobContinuation(tokens[i+1:], suffix)
		for _, alternative := range group.alts {
			fsGlobFillNegs(alternative, rest)
		}
	}
}

func fsGlobContinuation(after, suffix []fsGlobToken) []fsGlobToken {
	rest := make([]fsGlobToken, 0, len(after)+len(suffix))
	rest = append(rest, after...)
	return append(rest, suffix...)
}

func fsGlobHasNegative(alternatives [][]fsGlobToken) bool {
	for _, alternative := range alternatives {
		for _, token := range alternative {
			if token.alts == nil {
				continue
			}
			if token.op == '!' && !token.empty {
				return true
			}
			if fsGlobHasNegative(token.alts) {
				return true
			}
		}
	}
	return false
}

func fsGlobMatchJoined(first, last []fsGlobToken, name string, start, end int, nocase, unicodeCase bool) bool {
	for split := start; ; {
		if fsGlobMatchTokens(first, name, start, split, nocase, unicodeCase) && fsGlobMatchTokens(last, name, split, end, nocase, unicodeCase) {
			return true
		}
		if split == end {
			return false
		}
		_, size := fsGlobRune(name[split:end])
		split += size
	}
}

func (part fsGlobPart) match(name string) bool {
	if !part.magic {
		return part.literal == name
	}
	if name == "." || name == ".." {
		return false
	}
	nocase := runtime.GOOS == "windows" || runtime.GOOS == "darwin"
	if nocase {
		switch part.fast {
		case fsGlobFastStar:
			return len(name) != 0 && name[0] != '.'
		case fsGlobFastStarExt:
			if len(name) == 0 || name[0] == '.' {
				return false
			}
			return strings.HasSuffix(ToLowerCase(name).(string), part.extLower)
		case fsGlobFastQmarks:
			if fsGlobUTF16Units(name) != part.qmarks+part.extUnits || len(name) == 0 || name[0] == '.' {
				return false
			}
			return strings.HasSuffix(ToLowerCase(name).(string), part.extLower)
		}
	}
	if !part.unicode {
		name = fsGlobUTF16(name)
	}
	return fsGlobMatchTokens(part.tokens, name, 0, len(name), nocase, part.unicode)
}

func fsGlobMatchTokens(tokens []fsGlobToken, name string, start, limit int, nocase, unicodeCase bool) bool {
	if len(tokens) == 0 {
		return start == limit
	}
	token, rest := tokens[0], tokens[1:]
	if token.op == '!' && token.empty {
		// minimatch's `(?!\.)[^/]+?`: the body is ignored, one or more units.
		if start == 0 && start < limit && name[start] == '.' {
			return false
		}
		end := start
		for end < limit {
			_, size := fsGlobRune(name[end:limit])
			end += size
			if fsGlobMatchTokens(rest, name, end, limit, nocase, unicodeCase) {
				return true
			}
		}
		return false
	}
	if token.alts != nil {
		if token.op == '!' {
			if start == 0 && start < limit && name[start] == '.' {
				return false
			}
			for _, alternative := range token.alts {
				if fsGlobMatchJoined(alternative, token.tail, name, start, len(name), nocase, unicodeCase) {
					return false
				}
			}
		}
		for end := start; ; {
			if (token.op == '!' || fsGlobMatchGroup(token, name, start, end, nocase, unicodeCase)) && fsGlobMatchTokens(rest, name, end, limit, nocase, unicodeCase) {
				return true
			}
			if end >= limit {
				return false
			}
			_, size := fsGlobRune(name[end:limit])
			end += size
		}
	}
	if token.op == 'N' {
		// The `starNoEmpty` of a negative lookahead's tail: one or more units.
		end := start
		if end >= limit {
			return false
		}
		_, size := fsGlobRune(name[end:limit])
		end += size
		for {
			if fsGlobMatchTokens(rest, name, end, limit, nocase, unicodeCase) {
				return true
			}
			if end >= limit {
				return false
			}
			_, size = fsGlobRune(name[end:limit])
			end += size
		}
	}
	if token.op == '*' {
		if start == 0 && start < limit && name[start] == '.' {
			return false
		}
		for end := start; ; {
			if fsGlobMatchTokens(rest, name, end, limit, nocase, unicodeCase) {
				return true
			}
			if end >= limit {
				return false
			}
			_, size := fsGlobRune(name[end:limit])
			end += size
		}
	}
	if start >= limit {
		return false
	}
	ch, size := fsGlobRune(name[start:limit])
	matches := false
	switch token.op {
	case '=':
		matches = fsGlobEqual(ch, token.char, nocase, unicodeCase)
	case '?':
		matches = start != 0 || ch != '.'
	case '[':
		matches = (start != 0 || ch != '.') && token.class.match(ch, nocase, unicodeCase)
	}
	return matches && fsGlobMatchTokens(rest, name, start+size, limit, nocase, unicodeCase)
}

func fsGlobMatchGroup(group fsGlobToken, name string, start, end int, nocase, unicodeCase bool) bool {
	if group.op == '?' && start == end {
		return true
	}
	if group.op == '*' || group.op == '+' {
		return fsGlobMatchRepeat(group.alts, name, start, end, group.op == '+', nocase, unicodeCase)
	}
	for _, alternative := range group.alts {
		if fsGlobMatchTokens(alternative, name, start, end, nocase, unicodeCase) {
			return true
		}
	}
	return false
}

func fsGlobMatchRepeat(alternatives [][]fsGlobToken, name string, start, end int, required, nocase, unicodeCase bool) bool {
	if start == end {
		if !required {
			return true
		}
		for _, alternative := range alternatives {
			if fsGlobMatchTokens(alternative, name, start, end, nocase, unicodeCase) {
				return true
			}
		}
		return false
	}
	for split := start; split < end; {
		_, size := fsGlobRune(name[split:end])
		split += size
		for _, alternative := range alternatives {
			if fsGlobMatchTokens(alternative, name, start, split, nocase, unicodeCase) && fsGlobMatchRepeat(alternatives, name, split, end, false, nocase, unicodeCase) {
				return true
			}
		}
	}
	return false
}
