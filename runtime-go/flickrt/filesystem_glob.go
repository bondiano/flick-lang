package flickrt

import (
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// globPaths is Node's `fs.glob`: the paths below `cwd` matching the
// pattern (`*`, `?`, `[…]`, `{a,b}` and `**` across directories), names
// starting with a dot only by a pattern that starts with one, without
// those the exclusions match, which are not searched below either.
//
// shortcut: paths come sorted, where Node lists them in the order it reads the directories; read them in that order if a program depends on it
func globPaths(pattern, cwd string, exclude []string) ([]Value, error) {
	var excluded []*regexp.Regexp
	for _, e := range exclude {
		for _, p := range expandBraces(e) {
			excluded = append(excluded, globRegexp(p))
		}
	}
	isExcluded := func(rel string) bool {
		for _, e := range excluded {
			if e.MatchString(rel) {
				return true
			}
		}
		return false
	}
	found := map[string]bool{}
	for _, p := range expandBraces(pattern) {
		base, segments := cwd, strings.Split(filepath.ToSlash(p), "/")
		prefix := ""
		if filepath.IsAbs(p) {
			base, prefix = "/", "/"
			segments = segments[1:]
		}
		globWalk(base, prefix, segments, isExcluded, found)
	}
	out := make([]string, 0, len(found))
	for p := range found {
		out = append(out, p)
	}
	sort.Strings(out)
	values := make([]Value, len(out))
	for i, p := range out {
		values[i] = filepath.FromSlash(p)
	}
	return values, nil
}

// globWalk matches the segments in the directory `dir`, which `rel` names
// relative to where the search started.
func globWalk(dir, rel string, segments []string, excluded func(string) bool, found map[string]bool) {
	if len(segments) == 0 {
		if rel != "" && !excluded(strings.TrimSuffix(rel, "/")) {
			found[strings.TrimSuffix(rel, "/")] = true
		}
		return
	}
	seg, rest := segments[0], segments[1:]
	switch {
	case seg == "**":
		globWalk(dir, rel, rest, excluded, found)
		for _, e := range readNames(dir) {
			if strings.HasPrefix(e.Name(), ".") || !e.IsDir() || excluded(rel+e.Name()) {
				continue
			}
			globWalk(filepath.Join(dir, e.Name()), rel+e.Name()+"/", segments, excluded, found)
		}
	case seg == "":
		globWalk(dir, rel, rest, excluded, found)
	case !strings.ContainsAny(seg, "*?["):
		path := filepath.Join(dir, seg)
		if _, err := os.Lstat(path); err == nil {
			globWalk(path, rel+seg+"/", rest, excluded, found)
		}
	default:
		re := globRegexp(seg)
		for _, e := range readNames(dir) {
			name := e.Name()
			if strings.HasPrefix(name, ".") && !strings.HasPrefix(seg, ".") {
				continue
			}
			if re.MatchString(name) && !excluded(rel+name) {
				if len(rest) > 0 && !e.IsDir() {
					if fi, err := os.Stat(filepath.Join(dir, name)); err != nil || !fi.IsDir() {
						continue
					}
				}
				globWalk(filepath.Join(dir, name), rel+name+"/", rest, excluded, found)
			}
		}
	}
}

func readNames(dir string) []os.DirEntry {
	entries, _ := os.ReadDir(dir)
	return entries
}

// expandBraces is the patterns of `{a,b}` alternatives.
func expandBraces(p string) []string {
	depth, start := 0, -1
	for i := 0; i < len(p); i++ {
		switch p[i] {
		case '\\':
			i++
		case '{':
			if depth == 0 {
				start = i
			}
			depth++
		case '}':
			depth--
			if depth == 0 && start >= 0 {
				var alts []string
				d, from := 0, start+1
				for j := start + 1; j < i; j++ {
					switch p[j] {
					case '{':
						d++
					case '}':
						d--
					case ',':
						if d == 0 {
							alts = append(alts, p[from:j])
							from = j + 1
						}
					}
				}
				if alts == nil {
					continue
				}
				alts = append(alts, p[from:i])
				var out []string
				for _, a := range alts {
					out = append(out, expandBraces(p[:start]+a+p[i+1:])...)
				}
				return out
			}
		}
	}
	return []string{p}
}

// globRegexp is a pattern of one or more segments as a regular expression;
// `**` crosses directories.
func globRegexp(p string) *regexp.Regexp {
	var b strings.Builder
	b.WriteString("^")
	for i := 0; i < len(p); i++ {
		switch c := p[i]; c {
		case '*':
			if i+1 < len(p) && p[i+1] == '*' {
				b.WriteString(".*")
				i++
				if i+1 < len(p) && p[i+1] == '/' {
					i++
					b.WriteString("/?")
				}
			} else {
				b.WriteString("[^/]*")
			}
		case '?':
			b.WriteString("[^/]")
		case '[':
			end := strings.IndexByte(p[i+1:], ']')
			if end < 0 {
				b.WriteString(`\[`)
				continue
			}
			class := p[i+1 : i+1+end]
			if strings.HasPrefix(class, "!") {
				class = "^" + class[1:]
			}
			b.WriteString("[" + strings.ReplaceAll(class, `\`, `\\`) + "]")
			i += end + 1
		case '\\':
			if i+1 < len(p) {
				i++
				b.WriteString(regexp.QuoteMeta(p[i : i+1]))
			}
		default:
			b.WriteString(regexp.QuoteMeta(string(c)))
		}
	}
	b.WriteString("$")
	re, err := regexp.Compile(b.String())
	if err != nil {
		return regexp.MustCompile("^" + regexp.QuoteMeta(p) + "$")
	}
	return re
}
