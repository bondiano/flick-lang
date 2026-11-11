package flickrt

// Brace expansion of glob patterns: `{a,b}` groups that may nest, skipping
// bracket classes; a pattern whose expansion is too deep or too large stays
// literal.

type fsGlobBraceGroup struct {
	commas []int
	close  int
}

// fsGlobBracePart is literal text or, when `alternatives` is set, a group.
type fsGlobBracePart struct {
	text         string
	alternatives [][]fsGlobBracePart
}

func fsGlobExpandBraces(pattern string) []string {
	chars := fsGlobChars(pattern)
	groups := fsGlobBraceGroups(chars)
	if len(groups) == 0 {
		return []string{pattern}
	}
	sequence, ok := fsGlobBraceSequence(chars, 0, len(chars), groups, 0)
	if !ok {
		return []string{pattern}
	}
	count := fsGlobCountExpansions(sequence)
	if count > fsGlobMaxExpansions || count*utf16Len(pattern) > fsGlobMaxExpandedLength {
		return []string{pattern}
	}
	return fsGlobExpandSequence(sequence)
}

// fsGlobBraceGroups indexes the expandable groups by opening position.
func fsGlobBraceGroups(chars []rune) map[int]fsGlobBraceGroup {
	groups := map[int]fsGlobBraceGroup{}
	type open struct {
		at     int
		commas []int
	}
	stack := []open{}
	classes := true
	for i := 0; i < len(chars); i++ {
		switch chars[i] {
		case '\\':
			i++
		case '/':
			classes = true
		case '[':
			if classes {
				if _, end, ok := fsGlobParseClass(chars, i); ok {
					i = end
				} else {
					classes = false
				}
			}
		case '{':
			stack = append(stack, open{at: i})
		case ',':
			if n := len(stack); n > 0 {
				stack[n-1].commas = append(stack[n-1].commas, i)
			}
		case '}':
			if n := len(stack); n > 0 {
				group := stack[n-1]
				stack = stack[:n-1]
				if len(group.commas) > 0 {
					groups[group.at] = fsGlobBraceGroup{commas: group.commas, close: i}
				}
			}
		}
	}
	return groups
}

func fsGlobBraceSequence(chars []rune, start, end int, groups map[int]fsGlobBraceGroup, depth int) ([]fsGlobBracePart, bool) {
	if depth > fsGlobMaxBraceDepth {
		return nil, false
	}
	parts := []fsGlobBracePart{}
	text := []byte{}
	for i := start; i < end; i++ {
		group, ok := groups[i]
		if !ok {
			if chars[i] == '\\' && i+1 < end {
				text = encodeRune(text, chars[i])
				i++
			}
			text = encodeRune(text, chars[i])
			continue
		}
		alternatives := [][]fsGlobBracePart{}
		from := i + 1
		for k := 0; k <= len(group.commas); k++ {
			to := group.close
			if k < len(group.commas) {
				to = group.commas[k]
			}
			alternative, ok := fsGlobBraceSequence(chars, from, to, groups, depth+1)
			if !ok {
				return nil, false
			}
			alternatives = append(alternatives, alternative)
			from = to + 1
		}
		if len(text) > 0 {
			parts = append(parts, fsGlobBracePart{text: string(text)})
			text = text[:0]
		}
		parts = append(parts, fsGlobBracePart{alternatives: alternatives})
		i = group.close
	}
	if len(text) > 0 {
		parts = append(parts, fsGlobBracePart{text: string(text)})
	}
	return parts, true
}

// fsGlobCountExpansions caps at one past the limit.
func fsGlobCountExpansions(sequence []fsGlobBracePart) int {
	count := 1
	for _, part := range sequence {
		if part.alternatives != nil {
			sum := 0
			for _, alternative := range part.alternatives {
				sum += fsGlobCountExpansions(alternative)
			}
			count *= sum
			if count > fsGlobMaxExpansions {
				return fsGlobMaxExpansions + 1
			}
		}
	}
	return count
}

func fsGlobExpandSequence(sequence []fsGlobBracePart) []string {
	out := []string{""}
	for _, part := range sequence {
		if part.alternatives == nil {
			for i := range out {
				out[i] += part.text
			}
			continue
		}
		alternatives := []string{}
		for _, alternative := range part.alternatives {
			alternatives = append(alternatives, fsGlobExpandSequence(alternative)...)
		}
		next := make([]string, 0, len(out)*len(alternatives))
		for _, prefix := range out {
			for _, alternative := range alternatives {
				next = append(next, prefix+alternative)
			}
		}
		out = next
	}
	return out
}
