package flickrt

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
)

const fsGlobExpansionLimit = 100000

type fsGlobPattern struct {
	base      string
	output    string
	parts     []fsGlobPart
	directory bool
	dotRoot   bool
}

func fsGlobPatternLength(pattern string) int {
	n := 0
	for _, ch := range pattern {
		n++
		if ch > 0xffff {
			n++
		}
	}
	return n
}

func fsGlobCompile(pattern, root string) []fsGlobPattern {
	if fsGlobPatternLength(pattern) > 65536 {
		Fail(fsBadArgument("glob", "pattern is too long"))
	}
	pattern = strings.ReplaceAll(pattern, "\\", "/")
	if pattern == "" {
		return nil
	}
	out := []fsGlobPattern{}
	for _, expanded := range fsGlobUnique(fsGlobBraces(pattern, true)) {
		p := fsGlobPattern{base: root, output: ".", directory: strings.HasSuffix(expanded, "/")}
		if runtime.GOOS == "windows" && strings.HasPrefix(expanded, "/") && filepath.VolumeName(expanded) == "" {
			// Node's win32.resolve roots a separator-only path on the current
			// drive, but keeps the emitted result root-relative.
			p.base = filepath.VolumeName(root) + string(filepath.Separator)
			p.output = string(filepath.Separator)
			expanded = strings.TrimPrefix(expanded, "/")
		} else if filepath.IsAbs(expanded) {
			p.base = filepath.VolumeName(expanded) + string(filepath.Separator)
			p.output = p.base
			expanded = strings.TrimPrefix(expanded, filepath.ToSlash(filepath.VolumeName(expanded)))
		}
		leading, pendingDot := true, false
		for _, segment := range strings.Split(expanded, "/") {
			if segment == "" {
				continue
			}
			if segment == "." {
				if leading {
					p.dotRoot = true
				}
				continue
			}
			leading = false
			part := fsGlobCompilePart(segment)
			// minimatch's first phase cancels a single-segment part before
			// `..`, magic or literal, before the tree is walked.
			if segment == ".." && len(p.parts) != 0 && !p.parts[len(p.parts)-1].recursive && p.parts[len(p.parts)-1].literal != ".." {
				pendingDot = len(p.parts) == 1
				p.parts = p.parts[:len(p.parts)-1]
				continue
			}
			if pendingDot && part.recursive {
				p.dotRoot = true
			}
			pendingDot = false
			if part.recursive && len(p.parts) != 0 && p.parts[len(p.parts)-1].recursive {
				continue
			}
			p.parts = append(p.parts, part)
		}
		out = append(out, p)
	}
	return out
}

func fsGlobBracePair(pattern string) (int, int) {
	stack := []int{}
	firstLeft, firstRight := -1, -1
	for i := range len(pattern) {
		switch pattern[i] {
		case '{':
			stack = append(stack, i)
		case '}':
			if len(stack) != 0 {
				start := stack[len(stack)-1]
				stack = stack[:len(stack)-1]
				if firstLeft < 0 || start < firstLeft {
					firstLeft, firstRight = start, i
				}
				if len(stack) == 0 {
					return start, i
				}
			}
		}
	}
	return firstLeft, firstRight
}

func fsGlobBraces(pattern string, top bool) []string {
	if top && strings.HasPrefix(pattern, "{}") {
		// minimatch escapes a leading `{}` before it searches for a pair.
		out := []string{}
		for _, rest := range fsGlobBraces(pattern[2:], true) {
			out = append(out, "{}"+rest)
		}
		return out
	}
	left, right := fsGlobBracePair(pattern)
	if left < 0 {
		return []string{pattern}
	}
	prefix, body := pattern[:left], pattern[left+1:right]
	post := pattern[right+1:]
	if strings.HasSuffix(prefix, "$") {
		out := []string{}
		for _, tail := range fsGlobBraces(post, false) {
			out = append(out, prefix+"{"+body+"}"+tail)
		}
		return out
	}
	sequence, isSequence := fsGlobBraceRange(body)
	options := strings.Contains(body, ",")
	if !isSequence && !options {
		// minimatch escapes the first closing brace and reprocesses only when a
		// later group brings the comma that makes this one an option list.
		if !fsGlobPostHasCommaGroup(post) {
			return []string{pattern}
		}
		out := []string{}
		for _, tail := range fsGlobBraces(post, false) {
			out = append(out, prefix+"{"+body+"}"+tail)
		}
		return out
	}
	choices := []string{}
	if isSequence {
		choices = sequence
	} else if parts := fsGlobSplitCommas(body); len(parts) > 1 {
		for _, part := range parts {
			choices = append(choices, fsGlobBraces(part, false)...)
			if len(choices) >= fsGlobExpansionLimit {
				choices = choices[:fsGlobExpansionLimit]
				break
			}
		}
	} else {
		for _, choice := range fsGlobBraces(body, false) {
			choices = append(choices, "{"+choice+"}")
		}
	}
	tails := fsGlobBraces(post, false)
	out := []string{}
	for _, choice := range choices {
		for _, tail := range tails {
			value := prefix + choice + tail
			if !top || value != "" {
				out = append(out, value)
				if len(out) == fsGlobExpansionLimit {
					return out
				}
			}
		}
	}
	return out
}

// fsGlobPostHasCommaGroup is minimatch's `,(?!,).*\}`: a comma outside a
// doubled comma with a later closing brace, which makes recovery worthwhile.
func fsGlobPostHasCommaGroup(post string) bool {
	for i := 0; i < len(post); i++ {
		if post[i] == ',' && (i+1 == len(post) || post[i+1] != ',') && strings.Contains(post[i:], "}") {
			return true
		}
	}
	return false
}

// fsGlobUnique is minimatch's Set of expanded patterns: first occurrence wins,
// so repeated alternatives are compiled and walked once.
func fsGlobUnique(patterns []string) []string {
	seen := make(map[string]bool, len(patterns))
	out := patterns[:0]
	for _, pattern := range patterns {
		if !seen[pattern] {
			seen[pattern] = true
			out = append(out, pattern)
		}
	}
	return out
}

func fsGlobSplitCommas(body string) []string {
	parts, start, depth := []string{}, 0, 0
	for i := 0; i < len(body); i++ {
		switch body[i] {
		case '{':
			depth++
		case '}':
			depth--
		case ',':
			if depth == 0 {
				parts = append(parts, body[start:i])
				start = i + 1
			}
		}
	}
	return append(parts, body[start:])
}

func fsGlobBraceRange(body string) ([]string, bool) {
	parts := strings.Split(body, "..")
	if len(parts) < 2 || len(parts) > 3 {
		return nil, false
	}
	for _, part := range parts {
		if strings.HasPrefix(part, "+") {
			return nil, false
		}
	}
	first, errFirst := strconv.ParseInt(parts[0], 10, 64)
	last, errLast := strconv.ParseInt(parts[1], 10, 64)
	alphabetic := false
	if errFirst != nil || errLast != nil {
		if len(parts[0]) != 1 || len(parts[1]) != 1 || !fsGlobASCIIAlpha(parts[0][0]) || !fsGlobASCIIAlpha(parts[1][0]) {
			return nil, false
		}
		first, last, alphabetic = int64(parts[0][0]), int64(parts[1][0]), true
	}
	step := int64(1)
	if len(parts) == 3 {
		var err error
		step, err = strconv.ParseInt(parts[2], 10, 64)
		if err != nil || step == -1<<63 {
			return nil, false
		}
		if step < 0 {
			step = -step
		}
		step = max(step, 1)
	}
	padded := false
	if !alphabetic {
		// minimatch's isPadded tests every `..` field, the step included.
		for _, field := range parts {
			digits := strings.TrimPrefix(field, "-")
			if len(digits) > 1 && strings.HasPrefix(digits, "0") {
				padded = true
			}
		}
	}
	width := max(len(parts[0]), len(parts[1]))
	out := []string{}
	for n := first; ; {
		value := strconv.FormatInt(n, 10)
		if alphabetic {
			value = string(rune(n))
			if n == '\\' {
				value = ""
			}
		} else if padded {
			value = fmt.Sprintf("%0*d", width, n)
		}
		out = append(out, value)
		if len(out) == fsGlobExpansionLimit || n == last {
			break
		}
		if first < last {
			if uint64(last)-uint64(n) < uint64(step) {
				break
			}
			n += step
		} else {
			if uint64(n)-uint64(last) < uint64(step) {
				break
			}
			n -= step
		}
	}
	return out, true
}

func fsGlobASCIIAlpha(ch byte) bool {
	return ch >= 'a' && ch <= 'z' || ch >= 'A' && ch <= 'Z'
}

func (pattern fsGlobPattern) matches(path string, directory bool) bool {
	if pattern.directory && !directory {
		return false
	}
	relative, err := filepath.Rel(pattern.base, path)
	if err != nil {
		return false
	}
	parts := []string{}
	if relative != "." {
		parts = strings.Split(filepath.ToSlash(relative), "/")
	}
	return fsGlobMatchPath(pattern.parts, parts)
}

func fsGlobMatchPath(pattern []fsGlobPart, path []string) bool {
	if len(pattern) == 0 {
		return len(path) == 0
	}
	if pattern[0].recursive {
		if len(pattern) == 1 {
			// minimatch's #matchGlobstar reports whether it consumed anything
			// and rejects `.`/`..`/dot segments it would have to cross.
			if len(path) == 0 {
				return false
			}
			for _, segment := range path {
				if segment == "." || segment == ".." || strings.HasPrefix(segment, ".") {
					return false
				}
			}
			return true
		}
		if fsGlobMatchPath(pattern[1:], path) {
			return true
		}
		return len(path) != 0 && !strings.HasPrefix(path[0], ".") && fsGlobMatchPath(pattern, path[1:])
	}
	return len(path) != 0 && pattern[0].match(path[0]) && fsGlobMatchPath(pattern[1:], path[1:])
}

// covers is native's separate `${fullpath}/` pruning check: the entries below
// this directory are excluded even though the directory itself is a result.
func (pattern fsGlobPattern) covers(path string) bool {
	if len(pattern.parts) <= 1 || !pattern.parts[len(pattern.parts)-1].recursive {
		return false
	}
	relative, err := filepath.Rel(pattern.base, path)
	if err != nil {
		return false
	}
	parts := []string{}
	if relative != "." {
		parts = strings.Split(filepath.ToSlash(relative), "/")
	}
	return fsGlobMatchPath(pattern.parts[:len(pattern.parts)-1], parts)
}

func fsGlob(fb *Fiber, options Value, text string) Value {
	o := record(options)
	root := "."
	if value, ok := some(o.Get("root")); ok {
		root = value.(string)
	}
	root, err := filepath.Abs(root)
	if err != nil {
		return Fail(systemError(err, "glob", "getcwd", root))
	}
	patterns := fsGlobCompile(text, root)
	excludes := []fsGlobPattern{}
	for _, excluded := range List(o.Get("exclude")) {
		path := strings.ReplaceAll(excluded.(string), "\\", "/")
		if !filepath.IsAbs(path) {
			if runtime.GOOS == "windows" && strings.HasPrefix(path, "/") {
				path = filepath.VolumeName(root) + path
			} else {
				path = filepath.Join(root, path)
			}
		}
		// native resolves every exclude, absolute included, which strips a
		// trailing slash before the matcher is built.
		path = filepath.Clean(path)
		excludes = append(excludes, fsGlobCompile(path, root)...)
	}
	results := []Value{}
	seen := map[string]bool{}
	for _, pattern := range patterns {
		type state struct {
			path      string
			index     int
			literal   bool
			fromLink  bool
			afterStar bool
		}
		visited := map[state]bool{}
		var walk func(string, string, int, bool, bool, bool)
		walk = func(path, output string, index int, literal, fromLink, afterStar bool) {
			Check(fb)
			key := state{path, index, literal, fromLink, afterStar}
			if visited[key] {
				return
			}
			visited[key] = true
			info, err := os.Lstat(path)
			if err != nil {
				return
			}
			for _, excluded := range excludes {
				if excluded.matches(path, info.IsDir()) {
					return
				}
				// Native prunes a queued descent, not its root or a completed match.
				if info.IsDir() && path != pattern.base && index < len(pattern.parts) && excluded.covers(path) {
					return
				}
			}
			if index == len(pattern.parts) {
				if pattern.directory {
					followed, err := os.Stat(path)
					if err != nil || !followed.IsDir() {
						return
					}
				}
				if !seen[output] {
					seen[output] = true
					results = append(results, output)
				}
				return
			}
			part := pattern.parts[index]
			if !part.magic {
				walk(filepath.Join(path, part.literal), filepath.Join(output, part.literal), index+1, literal && !afterStar, false, afterStar)
				return
			}
			if part.recursive {
				walk(path, output, index+1, literal, fromLink, true)
			}
			// Only a literal prefix, or the explicit root that `./**` names,
			// opens a symlink; a globstar does not.
			if !info.IsDir() && !(info.Mode()&os.ModeSymlink != 0 && (literal || fromLink)) {
				return
			}
			entries, err := os.ReadDir(path)
			if err != nil {
				return
			}
			for _, entry := range entries {
				name := entry.Name()
				if part.recursive {
					if strings.HasPrefix(name, ".") {
						if index+1 < len(pattern.parts) && !pattern.parts[index+1].recursive && pattern.parts[index+1].match(name) && entry.IsDir() {
							walk(filepath.Join(path, name), filepath.Join(output, name), index+1, false, false, true)
						}
						continue
					}
					if pattern.dotRoot && entry.Type()&os.ModeSymlink != 0 {
						// `./**` reaches a symlinked directory once.
						walk(filepath.Join(path, name), filepath.Join(output, name), index+1, false, true, true)
					}
					if entry.IsDir() {
						walk(filepath.Join(path, name), filepath.Join(output, name), index, false, false, true)
					} else if index+1 == len(pattern.parts) {
						walk(filepath.Join(path, name), filepath.Join(output, name), index+1, false, false, true)
					}
				} else if part.match(name) && (index+1 == len(pattern.parts) || entry.IsDir()) {
					walk(filepath.Join(path, name), filepath.Join(output, name), index+1, false, false, afterStar)
				}
			}
		}
		walk(pattern.base, pattern.output, 0, len(pattern.parts) != 0 && !pattern.parts[0].magic, false, false)
	}
	return results
}
