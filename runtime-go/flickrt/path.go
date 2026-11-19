package flickrt

import (
	"os"
	"strings"
)

// Effect's `Path` on POSIX rules, a port of Node's `path.posix` as Effect
// ports it; paths are scanned by their ASCII separators and dots, so
// slicing a WTF-8 string at them is slicing the JS string.
//
// shortcut: `Path.node` is POSIX on windows too, where Node's is `path.win32` (drive letters, `\`); port win32 when a program runs paths through it there

// normalizeStringPosix resolves `.` and `..` in a path without its root.
func normalizeStringPosix(path string, allowAboveRoot bool) string {
	res := ""
	lastSegmentLength, lastSlash, dots := 0, -1, 0
	var code byte
	for i := 0; i <= len(path); i++ {
		if i < len(path) {
			code = path[i]
		} else if code == '/' {
			break
		} else {
			code = '/'
		}
		switch {
		case code == '/':
			switch {
			case lastSlash == i-1 || dots == 1:
			case dots == 2:
				if len(res) < 2 || lastSegmentLength != 2 || res[len(res)-1] != '.' || res[len(res)-2] != '.' {
					if len(res) > 2 {
						if k := strings.LastIndexByte(res, '/'); k != len(res)-1 {
							if k == -1 {
								res, lastSegmentLength = "", 0
							} else {
								res = res[:k]
								lastSegmentLength = len(res) - 1 - strings.LastIndexByte(res, '/')
							}
							lastSlash, dots = i, 0
							continue
						}
					} else if len(res) == 2 || len(res) == 1 {
						res, lastSegmentLength = "", 0
						lastSlash, dots = i, 0
						continue
					}
				}
				if allowAboveRoot {
					if len(res) > 0 {
						res += "/.."
					} else {
						res = ".."
					}
					lastSegmentLength = 2
				}
			default:
				if len(res) > 0 {
					res += "/" + path[lastSlash+1:i]
				} else {
					res = path[lastSlash+1 : i]
				}
				lastSegmentLength = i - lastSlash - 1
			}
			lastSlash, dots = i, 0
		case code == '.' && dots != -1:
			dots++
		default:
			dots = -1
		}
	}
	return res
}

func pathResolve(paths ...string) string {
	resolved, absolute := "", false
	for i := len(paths) - 1; i >= -1 && !absolute; i-- {
		var p string
		if i >= 0 {
			p = paths[i]
		} else {
			p, _ = os.Getwd()
		}
		if p == "" {
			continue
		}
		resolved = p + "/" + resolved
		absolute = p[0] == '/'
	}
	resolved = normalizeStringPosix(resolved, !absolute)
	switch {
	case absolute:
		return "/" + resolved
	case resolved != "":
		return resolved
	}
	return "."
}

func pathNormalize(p string) string {
	if p == "" {
		return "."
	}
	absolute, trailing := p[0] == '/', p[len(p)-1] == '/'
	p = normalizeStringPosix(p, !absolute)
	if p == "" && !absolute {
		p = "."
	}
	if p != "" && trailing {
		p += "/"
	}
	if absolute {
		return "/" + p
	}
	return p
}

func pathJoin(paths []Value) string {
	joined, any := "", false
	for _, v := range paths {
		if s := v.(string); s != "" {
			if any {
				joined += "/" + s
			} else {
				joined, any = s, true
			}
		}
	}
	if !any {
		return "."
	}
	return pathNormalize(joined)
}

func pathRelative(from, to string) string {
	if from == to {
		return ""
	}
	from, to = pathResolve(from), pathResolve(to)
	if from == to {
		return ""
	}
	fromStart := 1
	for fromStart < len(from) && from[fromStart] == '/' {
		fromStart++
	}
	fromLen := len(from) - fromStart
	toStart := 1
	for toStart < len(to) && to[toStart] == '/' {
		toStart++
	}
	toLen := len(to) - toStart
	length := min(fromLen, toLen)
	lastCommonSep, i := -1, 0
	for ; i <= length; i++ {
		if i == length {
			if toLen > length {
				if to[toStart+i] == '/' {
					return to[toStart+i+1:]
				} else if i == 0 {
					return to[toStart+i:]
				}
			} else if fromLen > length {
				if from[fromStart+i] == '/' {
					lastCommonSep = i
				} else if i == 0 {
					lastCommonSep = 0
				}
			}
			break
		}
		if from[fromStart+i] != to[toStart+i] {
			break
		} else if from[fromStart+i] == '/' {
			lastCommonSep = i
		}
	}
	out := ""
	for i = fromStart + lastCommonSep + 1; i <= len(from); i++ {
		if i == len(from) || from[i] == '/' {
			if out == "" {
				out += ".."
			} else {
				out += "/.."
			}
		}
	}
	if out != "" {
		return out + to[toStart+lastCommonSep:]
	}
	toStart += lastCommonSep
	if toStart < len(to) && to[toStart] == '/' {
		toStart++
	}
	return to[toStart:]
}

func pathDirname(p string) string {
	if p == "" {
		return "."
	}
	hasRoot, end, matchedSlash := p[0] == '/', -1, true
	for i := len(p) - 1; i >= 1; i-- {
		if p[i] == '/' {
			if !matchedSlash {
				end = i
				break
			}
		} else {
			matchedSlash = false
		}
	}
	switch {
	case end == -1 && hasRoot:
		return "/"
	case end == -1:
		return "."
	case hasRoot && end == 1:
		return "//"
	}
	return p[:end]
}

func pathBasename(p, ext string) string {
	start, end, matchedSlash := 0, -1, true
	if ext != "" && len(ext) <= len(p) {
		if ext == p {
			return ""
		}
		extIdx, firstNonSlashEnd := len(ext)-1, -1
		for i := len(p) - 1; i >= 0; i-- {
			if p[i] == '/' {
				if !matchedSlash {
					start = i + 1
					break
				}
				continue
			}
			if firstNonSlashEnd == -1 {
				matchedSlash = false
				firstNonSlashEnd = i + 1
			}
			if extIdx >= 0 {
				if p[i] == ext[extIdx] {
					if extIdx--; extIdx == -1 {
						end = i
					}
				} else {
					extIdx, end = -1, firstNonSlashEnd
				}
			}
		}
		if start == end {
			end = firstNonSlashEnd
		} else if end == -1 {
			end = len(p)
		}
		return p[start:end]
	}
	for i := len(p) - 1; i >= 0; i-- {
		if p[i] == '/' {
			if !matchedSlash {
				start = i + 1
				break
			}
		} else if end == -1 {
			matchedSlash = false
			end = i + 1
		}
	}
	if end == -1 {
		return ""
	}
	return p[start:end]
}

// pathParts is Node's scan for the last segment of a path from `start`:
// where it starts, where it ends and where its extension starts.
func pathParts(p string, start int) (startPart, end, startDot int) {
	startDot, end = -1, -1
	matchedSlash, preDotState := true, 0
	for i := len(p) - 1; i >= start; i-- {
		code := p[i]
		if code == '/' {
			if !matchedSlash {
				startPart = i + 1
				break
			}
			continue
		}
		if end == -1 {
			matchedSlash = false
			end = i + 1
		}
		if code == '.' {
			if startDot == -1 {
				startDot = i
			} else if preDotState != 1 {
				preDotState = 1
			}
		} else if startDot != -1 {
			preDotState = -1
		}
	}
	if startDot == -1 || end == -1 || preDotState == 0 ||
		(preDotState == 1 && startDot == end-1 && startDot == startPart+1) {
		startDot = -1
	}
	return startPart, end, startDot
}

func pathExtname(p string) string {
	_, end, startDot := pathParts(p, 0)
	if startDot == -1 {
		return ""
	}
	return p[startDot:end]
}

func pathParse(p string) Value {
	root, dir, base, ext, name := "", "", "", "", ""
	if p != "" {
		absolute := p[0] == '/'
		start := 0
		if absolute {
			root, start = "/", 1
		}
		startPart, end, startDot := pathParts(p, start)
		from := startPart
		if startPart == 0 && absolute {
			from = 1
		}
		if startDot == -1 {
			if end != -1 {
				base, name = p[from:end], p[from:end]
			}
		} else {
			name, base, ext = p[from:startDot], p[from:end], p[startDot:end]
		}
		if startPart > 0 {
			dir = p[:startPart-1]
		} else if absolute {
			dir = "/"
		}
	}
	return Rec("root", root, "dir", dir, "base", base, "ext", ext, "name", name)
}

func pathFormat(parsed Value) string {
	o := record(parsed)
	dir := o.Get("dir").(string)
	if dir == "" {
		dir = o.Get("root").(string)
	}
	base := o.Get("base").(string)
	if base == "" {
		base = o.Get("name").(string) + o.Get("ext").(string)
	}
	switch {
	case dir == "":
		return base
	case dir == o.Get("root").(string):
		return dir + base
	}
	return dir + "/" + base
}

// pathResolveWithin is the input resolved under the root, none when it is
// absolute or leads out of the root.
func pathResolveWithin(root, input string) Value {
	if record(pathParse(input)).Get("root") != "" {
		return None
	}
	base := pathResolve(root)
	candidate := pathResolve(base, input)
	rel := pathRelative(base, candidate)
	if rel == ".." || strings.HasPrefix(rel, "../") || strings.HasPrefix(rel, "/") {
		return None
	}
	return Some(candidate)
}
