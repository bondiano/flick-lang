package flickrt

// Node's win32 paths (lib/path.js of Node 24), which `Path.node` is on
// Windows: either separator, drive letters, UNC and device roots, and
// reserved device names. As in path.go, the characters the code looks at
// are ASCII, so byte positions cut the text where JS's code units do.

import (
	"os"
	"runtime"
	"strings"
	"unicode/utf8"
)

func isWinSep(c byte) bool { return c == '/' || c == '\\' }

func isDriveLetter(c byte) bool { return c|0x20 >= 'a' && c|0x20 <= 'z' }

// hasDrive is whether `path` starts with a drive like `C:`.
func hasDrive(path string) bool { return len(path) >= 2 && isDriveLetter(path[0]) && path[1] == ':' }

func jsLower(s string) string { return ToLowerCase(s).(string) }

var winReservedNames = map[string]bool{}

func init() {
	for _, n := range strings.Fields("CON PRN AUX NUL COM1 COM2 COM3 COM4 COM5 COM6 COM7 COM8 COM9 LPT1 LPT2 LPT3 LPT4 LPT5 LPT6 LPT7 LPT8 LPT9 COM¹ COM² COM³ LPT¹ LPT² LPT³") {
		winReservedNames[n] = true
	}
}

// isWinReserved is Node's isWindowsReservedName: whether `path` up to
// `colon` names a device. A `colon` of -1 drops the last code unit, as
// JS's `slice(0, -1)`.
func isWinReserved(path string, colon int) bool {
	if colon < 0 {
		r, size := utf8.DecodeLastRuneInString(path)
		if r >= 0x10000 {
			return false // JS keeps half a surrogate pair, which names no device
		}
		colon = len(path) - size
	}
	return winReservedNames[ToUpperCase(path[:colon]).(string)]
}

// win32UNC matches `\\server\share` at the start of `path`, with either
// separator: the end of the server, and the start and end of the share.
func win32UNC(path string) (server, shareStart, shareEnd int, ok bool) {
	j := 2
	for j < len(path) && !isWinSep(path[j]) {
		j++
	}
	if j >= len(path) || j == 2 {
		return
	}
	server = j
	for j < len(path) && isWinSep(path[j]) {
		j++
	}
	if j >= len(path) || j == server {
		return
	}
	shareStart = j
	for j < len(path) && !isWinSep(path[j]) {
		j++
	}
	return server, shareStart, j, true
}

// win32NormalizeString is Node's normalizeString with `\` as separator.
func win32NormalizeString(path string, allowAboveRoot bool) string {
	res, lastSegment, lastSlash, dots := "", 0, -1, 0
	var code byte
	for i := 0; i <= len(path); i++ {
		if i < len(path) {
			code = path[i]
		} else if isWinSep(code) {
			break
		} else {
			code = '/'
		}
		if !isWinSep(code) {
			if code == '.' && dots != -1 {
				dots++
			} else {
				dots = -1
			}
			continue
		}
		switch {
		case lastSlash == i-1 || dots == 1:
		case dots == 2:
			if len(res) < 2 || lastSegment != 2 || !strings.HasSuffix(res, "..") {
				if len(res) > 2 {
					if last := len(res) - lastSegment - 1; last == -1 {
						res, lastSegment = "", 0
					} else {
						res = res[:last]
						lastSegment = len(res) - 1 - strings.LastIndexByte(res, '\\')
					}
					lastSlash, dots = i, 0
					continue
				} else if len(res) != 0 {
					res, lastSegment, lastSlash, dots = "", 0, i, 0
					continue
				}
			}
			if allowAboveRoot {
				if len(res) > 0 {
					res += `\..`
				} else {
					res = ".."
				}
				lastSegment = 2
			}
		default:
			if len(res) > 0 {
				res += `\` + path[lastSlash+1:i]
			} else {
				res = path[lastSlash+1 : i]
			}
			lastSegment = i - lastSlash - 1
		}
		lastSlash, dots = i, 0
	}
	return res
}

// win32WorkingDir is the working directory of `device`, or of the process
// for "": Windows keeps one per drive in the variable `=C:`. A variable,
// so the corpus test reads the same directory on every OS.
var win32WorkingDir = func(device string) string {
	if device != "" {
		if dir := os.Getenv("=" + device); dir != "" {
			return dir
		}
	}
	cwd, _ := os.Getwd()
	return cwd
}

func win32Resolve(args ...string) string {
	device, tail, absolute := "", "", false
	for i := len(args) - 1; i >= -1; i-- {
		var path string
		switch {
		case i >= 0:
			if path = args[i]; path == "" {
				continue
			}
		case device == "":
			path = win32WorkingDir("")
			if len(args) == 0 || len(args) == 1 && (args[0] == "" || args[0] == ".") && path != "" && isWinSep(path[0]) {
				if runtime.GOOS != "windows" {
					path = strings.ReplaceAll(path, "/", `\`)
				}
				return path
			}
		default:
			path = win32WorkingDir(device)
			if len(path) > 2 && path[2] == '\\' && !strings.EqualFold(path[:2], device) {
				path = device + `\`
			}
		}
		rootEnd, own, isAbsolute := 0, "", false
		switch {
		case len(path) == 1:
			if isWinSep(path[0]) {
				rootEnd, isAbsolute = 1, true
			}
		case isWinSep(path[0]):
			isAbsolute, rootEnd = true, 1
			if isWinSep(path[1]) {
				if server, start, end, ok := win32UNC(path); ok {
					if first := path[2:server]; first != "." && first != "?" {
						own, rootEnd = `\\`+first+`\`+path[start:end], end
					} else {
						own, rootEnd = `\\`+first, 4
					}
				} else {
					rootEnd = 0
				}
			}
		case hasDrive(path):
			own, rootEnd = path[:2], 2
			if len(path) > 2 && isWinSep(path[2]) {
				isAbsolute, rootEnd = true, 3
			}
		}
		if own != "" {
			if device == "" {
				device = own
			} else if jsLower(own) != jsLower(device) {
				continue // a path on another device
			}
		}
		if absolute {
			if device != "" {
				break
			}
		} else {
			tail, absolute = path[rootEnd:]+`\`+tail, isAbsolute
			if isAbsolute && device != "" {
				break
			}
		}
	}
	tail = win32NormalizeString(tail, !absolute)
	if absolute {
		return device + `\` + tail
	}
	if device+tail == "" {
		return "."
	}
	return device + tail
}

func win32Normalize(path string) string {
	n := len(path)
	if n == 0 {
		return "."
	}
	if n == 1 {
		if path[0] == '/' {
			return `\`
		}
		return path
	}
	rootEnd, device, hasDevice, absolute := 0, "", false, false
	if isWinSep(path[0]) {
		absolute, rootEnd = true, 1
		if isWinSep(path[1]) {
			rootEnd = 0
			if server, start, end, ok := win32UNC(path); ok {
				switch first := path[2:server]; {
				case first == "." || first == "?":
					device, hasDevice, rootEnd = `\\`+first, true, 4
					possible := ""
					if colon := strings.IndexByte(path, ':'); colon+1 > 4 {
						possible = path[4 : colon+1]
					}
					if possible != "" && isWinReserved(possible, len(possible)-1) {
						device, rootEnd = `\\?\`+possible, 4+len(possible)
					}
				case end == n:
					return `\\` + first + `\` + path[start:] + `\`
				default:
					device, hasDevice, rootEnd = `\\`+first+`\`+path[start:end], true, end
				}
			}
		}
	} else if colon := strings.IndexByte(path, ':'); colon > 0 {
		if isDriveLetter(path[0]) && colon == 1 {
			device, hasDevice, rootEnd = path[:2], true, 2
			if n > 2 && isWinSep(path[2]) {
				absolute, rootEnd = true, 3
			}
		} else if isWinReserved(path, colon) {
			device, hasDevice, rootEnd = path[:colon+1], true, colon+1
		}
	}
	tail := ""
	if rootEnd < n {
		tail = win32NormalizeString(path[rootEnd:], !absolute)
	}
	if tail == "" && !absolute {
		tail = "."
	}
	if tail != "" && isWinSep(path[n-1]) {
		tail += `\`
	}
	if !absolute && !hasDevice && strings.Contains(path, ":") {
		// a relative tail must not read as a drive (CVE-2024-36139)
		if hasDrive(tail) {
			return `.\` + tail
		}
		for i := 0; i < n; i++ {
			if path[i] == ':' && (i == n-1 || isWinSep(path[i+1])) {
				return `.\` + tail
			}
		}
	}
	if isWinReserved(path, strings.IndexByte(path, ':')) {
		return `.\` + device + tail
	}
	switch {
	case !hasDevice && absolute:
		return `\` + tail
	case !hasDevice:
		return tail
	case absolute:
		return device + `\` + tail
	}
	return device + tail
}

func win32IsAbsolute(path string) bool {
	return path != "" && (isWinSep(path[0]) || len(path) > 2 && hasDrive(path) && isWinSep(path[2]))
}

func win32Join(args []string) string {
	parts := []string{}
	for _, a := range args {
		if a != "" {
			parts = append(parts, a)
		}
	}
	if len(parts) == 0 {
		return "."
	}
	first, joined := parts[0], strings.Join(parts, `\`)
	// two leading separators would read as UNC, unless the first part
	// clearly is one: exactly two separators and then a name
	replace, slashes := true, 0
	if isWinSep(first[0]) {
		slashes++
		if len(first) > 1 && isWinSep(first[1]) {
			slashes++
			if len(first) > 2 {
				if isWinSep(first[2]) {
					slashes++
				} else {
					replace = false
				}
			}
		}
	}
	if replace {
		for slashes < len(joined) && isWinSep(joined[slashes]) {
			slashes++
		}
		if slashes >= 2 {
			joined = `\` + joined[slashes:]
		}
	}
	// a part naming a reserved device skips normalization
	for _, p := range strings.Split(joined, `\`) {
		if colon := strings.IndexByte(p, ':'); colon != -1 && isWinReserved(p, colon) {
			return strings.ReplaceAll(joined, "/", `\`)
		}
	}
	return win32Normalize(joined)
}
