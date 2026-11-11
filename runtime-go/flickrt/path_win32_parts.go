package flickrt

// The rest of Node's win32 paths (see path_win32.go), its file URLs, and
// `Path.node` on Windows.

import "strings"

func win32Relative(from, to string) string {
	if from == to {
		return ""
	}
	fromOrig, toOrig := win32Resolve(from), win32Resolve(to)
	if fromOrig == toOrig {
		return ""
	}
	from, to = jsLower(fromOrig), jsLower(toOrig)
	if from == to {
		return ""
	}
	// JS compares by segment once lowercasing changes a length in code
	// units; positions here are bytes, so a change in bytes does too
	if len(fromOrig) != len(from) || len(toOrig) != len(to) ||
		Length(fromOrig) != Length(from) || Length(toOrig) != Length(to) {
		fromSplit, toSplit := strings.Split(fromOrig, `\`), strings.Split(toOrig, `\`)
		if fromSplit[len(fromSplit)-1] == "" {
			fromSplit = fromSplit[:len(fromSplit)-1]
		}
		if toSplit[len(toSplit)-1] == "" {
			toSplit = toSplit[:len(toSplit)-1]
		}
		n := min(len(fromSplit), len(toSplit))
		i := 0
		for i < n && jsLower(fromSplit[i]) == jsLower(toSplit[i]) {
			i++
		}
		switch {
		case i == 0:
			return toOrig
		case i < n:
			return strings.Repeat(`..\`, len(fromSplit)-i) + strings.Join(toSplit[i:], `\`)
		case len(toSplit) > n:
			return strings.Join(toSplit[i:], `\`)
		case len(fromSplit) > n:
			return strings.Repeat(`..\`, len(fromSplit)-1-i) + ".."
		}
		return ""
	}
	fromStart, fromEnd := trimBackslashes(from)
	toStart, toEnd := trimBackslashes(to)
	fromLen, toLen := fromEnd-fromStart, toEnd-toStart
	n := min(fromLen, toLen)
	lastCommonSep, i := -1, 0
	for ; i < n; i++ {
		c := from[fromStart+i]
		if c != to[toStart+i] {
			break
		} else if c == '\\' {
			lastCommonSep = i
		}
	}
	if i != n {
		if lastCommonSep == -1 {
			return toOrig
		}
	} else {
		if toLen > n {
			if to[toStart+i] == '\\' {
				return toOrig[toStart+i+1:] // `from` is a base of `to`
			}
			if i == 2 {
				return toOrig[toStart+i:] // `from` is a device root
			}
		}
		if fromLen > n {
			if from[fromStart+i] == '\\' {
				lastCommonSep = i
			} else if i == 2 {
				lastCommonSep = 3
			}
		}
		if lastCommonSep == -1 {
			lastCommonSep = 0
		}
	}
	out := ""
	for i = fromStart + lastCommonSep + 1; i <= fromEnd; i++ {
		if i == fromEnd || from[i] == '\\' {
			if out == "" {
				out = ".."
			} else {
				out += `\..`
			}
		}
	}
	toStart += lastCommonSep
	if out != "" {
		return out + toOrig[toStart:toEnd]
	}
	if toStart < len(toOrig) && toOrig[toStart] == '\\' {
		toStart++
	}
	return toOrig[toStart:toEnd]
}

// trimBackslashes is where `path` starts and ends without its leading
// backslashes and, but for one character, its trailing ones.
func trimBackslashes(path string) (start, end int) {
	for start < len(path) && path[start] == '\\' {
		start++
	}
	end = len(path)
	for end-1 > start && path[end-1] == '\\' {
		end--
	}
	return
}

func win32ToNamespacedPath(path string) string {
	if path == "" {
		return path
	}
	resolved := win32Resolve(path)
	switch {
	case len(resolved) <= 2:
		return path
	case resolved[0] == '\\':
		if resolved[1] == '\\' && resolved[2] != '?' && resolved[2] != '.' {
			return `\\?\UNC\` + resolved[2:]
		}
	case hasDrive(resolved) && resolved[2] == '\\':
		return `\\?\` + resolved
	}
	return resolved
}

func win32Dirname(path string) string {
	n := len(path)
	if n == 0 {
		return "."
	}
	if n == 1 {
		if isWinSep(path[0]) {
			return path
		}
		return "."
	}
	rootEnd, offset := -1, 0
	if isWinSep(path[0]) {
		rootEnd, offset = 1, 1
		if isWinSep(path[1]) {
			if _, _, end, ok := win32UNC(path); ok {
				if end == n {
					return path // a UNC root only
				}
				rootEnd, offset = end+1, end+1
			}
		}
	} else if hasDrive(path) {
		rootEnd = 2
		if n > 2 && isWinSep(path[2]) {
			rootEnd = 3
		}
		offset = rootEnd
	}
	end, matchedSlash := -1, true
	for i := n - 1; i >= offset; i-- {
		if !isWinSep(path[i]) {
			matchedSlash = false
		} else if !matchedSlash {
			end = i
			break
		}
	}
	if end == -1 {
		if rootEnd == -1 {
			return "."
		}
		end = rootEnd
	}
	return path[:end]
}

func win32Basename(path, suffix string) string {
	start, end, matchedSlash := 0, -1, true
	if hasDrive(path) {
		start = 2
	}
	if suffix != "" && Length(suffix).(int32) <= Length(path).(int32) {
		if suffix == path {
			return ""
		}
		extIdx, firstNonSlashEnd := len(suffix)-1, -1
		for i := len(path) - 1; i >= start; i-- {
			if isWinSep(path[i]) {
				if !matchedSlash {
					start = i + 1
					break
				}
				continue
			}
			if firstNonSlashEnd == -1 {
				matchedSlash, firstNonSlashEnd = false, i+1
			}
			if extIdx >= 0 {
				if path[i] == suffix[extIdx] {
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
			end = len(path)
		}
		return path[start:end]
	}
	for i := len(path) - 1; i >= start; i-- {
		if isWinSep(path[i]) {
			if !matchedSlash {
				start = i + 1
				break
			}
		} else if end == -1 {
			matchedSlash, end = false, i+1
		}
	}
	if end == -1 {
		return ""
	}
	return path[start:end]
}

// win32Scan is the last part of `path` after `rootEnd`: where it starts,
// its extension's dot (-1 for none) and its end (-1 for none).
func win32Scan(path string, rootEnd int) (startPart, startDot, end int) {
	startPart, startDot, end = rootEnd, -1, -1
	matchedSlash, preDotState := true, 0
	for i := len(path) - 1; i >= rootEnd; i-- {
		c := path[i]
		if isWinSep(c) {
			if !matchedSlash {
				startPart = i + 1
				break
			}
			continue
		}
		if end == -1 {
			matchedSlash, end = false, i+1
		}
		if c == '.' {
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
		preDotState == 1 && startDot == end-1 && startDot == startPart+1 {
		startDot = -1
	}
	return
}

func win32Extname(path string) string {
	start := 0
	if hasDrive(path) {
		start = 2
	}
	_, dot, end := win32Scan(path, start)
	if dot == -1 {
		return ""
	}
	return path[dot:end]
}

func win32Parse(path string) Value {
	parsed := func(root, dir, base, ext, name string) Value {
		return Rec("root", root, "dir", dir, "base", base, "ext", ext, "name", name)
	}
	n := len(path)
	switch {
	case n == 0:
		return parsed("", "", "", "", "")
	case n == 1 && isWinSep(path[0]):
		return parsed(path, path, "", "", "")
	case n == 1:
		return parsed("", "", path, "", path)
	}
	rootEnd := 0
	if isWinSep(path[0]) {
		rootEnd = 1
		if isWinSep(path[1]) {
			if _, _, end, ok := win32UNC(path); ok {
				if rootEnd = end + 1; end == n {
					rootEnd = end
				}
			}
		}
	} else if hasDrive(path) {
		if n == 2 || n == 3 && isWinSep(path[2]) {
			return parsed(path, path, "", "", "")
		}
		rootEnd = 2
		if isWinSep(path[2]) {
			rootEnd = 3
		}
	}
	root, base, ext, name := path[:rootEnd], "", "", ""
	startPart, dot, end := win32Scan(path, rootEnd)
	if end != -1 {
		if base = path[startPart:end]; dot == -1 {
			name = base
		} else {
			name, ext = path[startPart:dot], path[dot:end]
		}
	}
	dir := root
	if startPart > 0 && startPart != rootEnd {
		dir = path[:startPart-1]
	}
	return parsed(root, dir, base, ext, name)
}

// win32Format is Node's `_format` with `\`, which adds the dot an `ext`
// lacks.
func win32Format(parsed Value) string {
	p := record(parsed)
	root, dir := p.Get("root").(string), p.Get("dir").(string)
	if dir == "" {
		dir = root
	}
	base := p.Get("base").(string)
	if base == "" {
		ext := p.Get("ext").(string)
		if ext != "" && ext[0] != '.' {
			ext = "." + ext
		}
		base = p.Get("name").(string) + ext
	}
	switch {
	case dir == "":
		return base
	case dir == root:
		return dir + base
	}
	return dir + `\` + base
}

// nodePathError is the BadArgument of NodePath, which carries Node's
// error as its cause and no description.
func nodePathError(method string) Value {
	return Fail(NewData(platformErrorTag, NewData(Tag("PlatformError.Reason.Bad"), Rec(
		"module_", "Path", "method", method, "description", None))))
}

// win32FromFileUrl is Node's fileURLToPath with `windows`.
// shortcut: a punycode (`xn--`) UNC host stays encoded where Node decodes it; decode it when a program reads such URLs
func win32FromFileUrl(url string) Value {
	u, ok := parseURL(url)
	if !ok {
		return Fail(pathBadArgument("fromFileUrl", "invalid URL "+url))
	}
	if u.scheme != "file" {
		return nodePathError("fromFileUrl")
	}
	lower := strings.ToLower(u.path)
	if strings.Contains(lower, "%2f") || strings.Contains(lower, "%5c") {
		return nodePathError("fromFileUrl")
	}
	path := strings.ReplaceAll(u.path, "/", `\`)
	if strings.Contains(path, "%") {
		if path, ok = decodeURIComponent(path); !ok {
			return nodePathError("fromFileUrl")
		}
	}
	if u.host != "" {
		return `\\` + u.host + path
	}
	if len(path) < 3 || !isDriveLetter(path[1]) || path[2] != ':' {
		return nodePathError("fromFileUrl")
	}
	return path[1:]
}

var win32URLChars = strings.NewReplacer("%", "%25", "\n", "%0A", "\r", "%0D", "\t", "%09", "?", "%3F", "#", "%23")

// win32ToFileUrl is Node's pathToFileURL with `windows`.
func win32ToFileUrl(path string) Value {
	unc := strings.HasPrefix(path, `\\`)
	resolved := path
	if !unc {
		resolved = win32Resolve(path)
	}
	host := ""
	if unc || strings.HasPrefix(resolved, `\\`) {
		prefix := 2
		if strings.HasPrefix(resolved, `\\?\UNC\`) {
			prefix = 8
		}
		end := strings.IndexByte(resolved[prefix:], '\\')
		if end <= 0 {
			return nodePathError("toFileUrl") // no resource path, or no server
		}
		if host, resolved = resolved[prefix:prefix+end], resolved[prefix+end:]; host == "?" {
			host = "" // `\\?\C:\a` is the path `C:\a`
		}
	} else if last := path[len(path)-1]; isWinSep(last) && !strings.HasSuffix(resolved, `\`) {
		resolved += "/" // resolve drops the trailing separator
	}
	encoded := strings.ReplaceAll(win32URLChars.Replace(resolved), `\`, "/")
	if !strings.HasPrefix(encoded, "/") {
		encoded = "/" + encoded
	}
	u, ok := parseURL("file://" + host + encoded)
	if !ok {
		return nodePathError("toFileUrl")
	}
	return u.href()
}

// PathWin32 is `Path.node` on Windows.
func PathWin32(key Value) Value {
	return LayerSucceed(key, Rec(
		"sep", Fn(func(*Fiber, Value) Value { return Succeed(`\`) }),
		"basename", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32Basename(p.(string), "")) }),
		"basename_without", Curry2(func(_ *Fiber, ext, p Value) Value { return Succeed(win32Basename(p.(string), ext.(string))) }),
		"dirname", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32Dirname(p.(string))) }),
		"extname", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32Extname(p.(string))) }),
		"format", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32Format(p)) }),
		"from_file_url", Fn(func(_ *Fiber, url Value) Value {
			return Eff(func(*Fiber) Value { return win32FromFileUrl(url.(string)) })
		}),
		"is_absolute", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32IsAbsolute(p.(string))) }),
		"join", Fn(func(_ *Fiber, ps Value) Value { return Succeed(win32Join(stringsOf(ps))) }),
		"normalize", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32Normalize(p.(string))) }),
		"parse", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32Parse(p.(string))) }),
		"relative", Curry2(func(_ *Fiber, from, to Value) Value { return Succeed(win32Relative(from.(string), to.(string))) }),
		"resolve", Fn(func(_ *Fiber, ps Value) Value {
			return Eff(func(*Fiber) Value { return win32Resolve(stringsOf(ps)...) })
		}),
		"resolve_within", Curry2(func(_ *Fiber, root, input Value) Value {
			return Eff(func(*Fiber) Value { return win32ResolveWithin(root.(string), input.(string)) })
		}),
		"to_file_url", Fn(func(_ *Fiber, p Value) Value {
			return Eff(func(*Fiber) Value { return win32ToFileUrl(p.(string)) })
		}),
		"to_namespaced_path", Fn(func(_ *Fiber, p Value) Value { return Succeed(win32ToNamespacedPath(p.(string))) }),
	))
}

// win32ResolveWithin is `input` resolved within `root`, over win32 paths.
func win32ResolveWithin(root, input string) Value {
	if record(win32Parse(input)).Get("root").(string) != "" {
		return None
	}
	base := win32Resolve(root)
	candidate := win32Resolve(base, input)
	rel := win32Relative(base, candidate)
	if rel == ".." || strings.HasPrefix(rel, `..\`) || win32IsAbsolute(rel) {
		return None
	}
	return Some(candidate)
}
