package flickrt

import (
	"strconv"
	"strings"
)

// webURL is a URL as the URL Standard's parser gives it, which JS's `URL`
// follows. It is a Go value of comparable fields, so `==` compares URLs as
// Effect's structural equality does; setters change a copy.
//
// shortcut: file URLs are parsed as a special scheme without the Windows drive-letter rules, and a non-ASCII host gets lower case and Punycode without the rest of UTS 46 (mapping, validity checks); port those if a program needs them
type webURL struct {
	scheme, username, password string
	host                       string
	hasHost                    bool
	port                       string // "" when the URL has none
	opaque                     bool   // a path like `mailto:` has, one string
	path                       string // the serialized path: `/a/b`, or an opaque one
	query, fragment            string
	hasQuery, hasFragment      bool
}

var specialPorts = map[string]string{
	"http": "80", "https": "443", "ws": "80", "wss": "443", "ftp": "21", "file": "",
}

func isSpecial(scheme string) bool { _, ok := specialPorts[scheme]; return ok }

// The percent-encode sets of the URL Standard.
func c0Set(c byte) bool { return c < 0x20 || c > 0x7E }
func fragmentSet(c byte) bool {
	return c0Set(c) || c == ' ' || c == '"' || c == '<' || c == '>' || c == '`'
}
func querySet(c byte) bool {
	return c0Set(c) || c == ' ' || c == '"' || c == '#' || c == '<' || c == '>'
}
func specialQuerySet(c byte) bool { return querySet(c) || c == '\'' }
func pathSet(c byte) bool {
	return querySet(c) || c == '?' || c == '^' || c == '`' || c == '{' || c == '}'
}
func userinfoSet(c byte) bool {
	return pathSet(c) || strings.IndexByte("/:;=@[\\]|", c) >= 0
}

// withoutTabs is the input with ASCII tabs and newlines left out, as every
// parse of a URL or of a setter's value begins.
func withoutTabs(s string) string {
	if !strings.ContainsAny(s, "\t\n\r") {
		return s
	}
	return strings.NewReplacer("\t", "", "\n", "", "\r", "").Replace(s)
}

func isAlpha(c byte) bool { return c|0x20 >= 'a' && c|0x20 <= 'z' }
func isDigit(c byte) bool { return c >= '0' && c <= '9' }
func isSlash(c byte) bool { return c == '/' || c == '\\' }

// schemeEnd is where a valid scheme at the start of `s` ends, or -1.
func schemeEnd(s string) int {
	if s == "" || !isAlpha(s[0]) {
		return -1
	}
	i := 1
	for i < len(s) && (isAlpha(s[i]) || isDigit(s[i]) || s[i] == '+' || s[i] == '-' || s[i] == '.') {
		i++
	}
	return i
}

// parseURL is the URL of an absolute URL text, as `new URL(text)`.
func parseURL(input string) (*webURL, bool) {
	input = withoutTabs(strings.TrimFunc(input, func(r rune) bool { return r <= ' ' }))
	end := schemeEnd(input)
	if end < 0 || end >= len(input) || input[end] != ':' {
		return nil, false
	}
	u := &webURL{scheme: strings.ToLower(input[:end])}
	rest := input[end+1:]
	special := isSpecial(u.scheme)
	rest, u.fragment, u.hasFragment = strings.Cut(rest, "#")
	if u.hasFragment {
		u.fragment = percentEncode(u.fragment, fragmentSet)
	}
	rest, u.query, u.hasQuery = strings.Cut(rest, "?")
	if u.hasQuery {
		u.query = percentEncode(u.query, u.querySet())
	}
	switch {
	case u.scheme == "file":
		if len(rest) < 2 || !isSlash(rest[0]) || !isSlash(rest[1]) {
			u.hasHost = true
			u.setPath(rest)
			return u, true
		}
		rest = rest[2:]
	case special:
		// a special URL has an authority however many slashes lead to it
		rest = strings.TrimLeft(rest, "/\\")
	case strings.HasPrefix(rest, "//"):
		rest = rest[2:]
	case strings.HasPrefix(rest, "/"):
		u.setPath(rest)
		return u, true
	default:
		u.opaque = true
		u.path = percentEncode(rest, c0Set)
		return u, true
	}
	cut := strings.IndexAny(rest, u.authorityEnds())
	if cut < 0 {
		cut = len(rest)
	}
	authority, path := rest[:cut], rest[cut:]
	if at := strings.LastIndexByte(authority, '@'); at >= 0 {
		user, pass, _ := strings.Cut(authority[:at], ":")
		u.username = percentEncode(user, userinfoSet)
		u.password = percentEncode(pass, userinfoSet)
		authority = authority[at+1:]
		if authority == "" {
			return nil, false
		}
	}
	host, port, hasPort := cutPort(authority)
	if !u.setHost(host) {
		return nil, false
	}
	if hasPort && !u.setPort(port, false) {
		return nil, false
	}
	if u.scheme == "file" && u.host == "localhost" {
		u.host = ""
	}
	u.setPath(path)
	return u, true
}

func (u webURL) querySet() func(byte) bool {
	if isSpecial(u.scheme) {
		return specialQuerySet
	}
	return querySet
}

func (u webURL) authorityEnds() string {
	if isSpecial(u.scheme) {
		return "/\\?#"
	}
	return "/?#"
}

// cutPort splits `host:port` at the first colon outside of brackets.
func cutPort(s string) (string, string, bool) {
	inside := false
	for i := 0; i < len(s); i++ {
		switch s[i] {
		case '[':
			inside = true
		case ']':
			inside = false
		case ':':
			if !inside {
				return s[:i], s[i+1:], true
			}
		}
	}
	return s, "", false
}

// setHost parses and sets the host; false, and no change, when it is
// none. A special URL other than `file:` needs one that is not empty.
func (u *webURL) setHost(s string) bool {
	special := isSpecial(u.scheme)
	if s == "" && special && u.scheme != "file" {
		return false
	}
	host, ok := parseHost(s, special)
	if !ok {
		return false
	}
	u.host, u.hasHost = host, true
	return true
}

// setPort sets the port of the leading digits of `s`; with `lenient`, as
// a setter, what follows them is ignored and no digits change nothing.
func (u *webURL) setPort(s string, lenient bool) bool {
	n := 0
	for n < len(s) && isDigit(s[n]) {
		n++
	}
	if n < len(s) && !lenient {
		return false
	}
	if n == 0 {
		return !lenient || s == ""
	}
	digits := strings.TrimLeft(s[:n], "0")
	if len(digits) > 5 {
		return false
	}
	port, _ := strconv.Atoi("0" + digits)
	if port > 65535 {
		return false
	}
	u.port = strconv.Itoa(port)
	if specialPorts[u.scheme] == u.port {
		u.port = ""
	}
	return true
}

// setPath sets the path of `s`, which starts with a slash or is empty.
func (u *webURL) setPath(s string) {
	special := isSpecial(u.scheme)
	u.path = ""
	if s == "" && !special {
		return
	}
	if s != "" && (s[0] == '/' || special && s[0] == '\\') {
		s = s[1:]
	}
	seps := "/"
	if special {
		seps = "/\\"
	}
	segments := splitKeep(s, seps)
	var path []string
	for i, seg := range segments {
		last := i == len(segments)-1
		switch dots(seg) {
		case 2:
			if len(path) > 0 {
				path = path[:len(path)-1]
			}
			if last {
				path = append(path, "")
			}
		case 1:
			if last {
				path = append(path, "")
			}
		default:
			path = append(path, percentEncode(seg, pathSet))
		}
	}
	if len(path) > 0 {
		u.path = "/" + strings.Join(path, "/")
	}
}

// splitKeep splits at each of `seps`, keeping empty parts.
func splitKeep(s, seps string) []string {
	var out []string
	for {
		i := strings.IndexAny(s, seps)
		if i < 0 {
			return append(out, s)
		}
		out = append(out, s[:i])
		s = s[i+1:]
	}
}

// dots is 1 for a segment that means `.`, 2 for `..`, else 0.
func dots(seg string) int {
	switch strings.ToLower(seg) {
	case ".", "%2e":
		return 1
	case "..", ".%2e", "%2e.", "%2e%2e":
		return 2
	}
	return 0
}

// href is the URL's text.
func (u webURL) href() string {
	var b strings.Builder
	b.WriteString(u.scheme + ":")
	if u.hasHost {
		b.WriteString("//")
		if u.username != "" || u.password != "" {
			b.WriteString(u.username)
			if u.password != "" {
				b.WriteString(":" + u.password)
			}
			b.WriteByte('@')
		}
		b.WriteString(u.host)
		if u.port != "" {
			b.WriteString(":" + u.port)
		}
	} else if !u.opaque && strings.HasPrefix(u.path, "//") {
		b.WriteString("/.")
	}
	b.WriteString(u.path)
	if u.hasQuery {
		b.WriteString("?" + u.query)
	}
	if u.hasFragment {
		b.WriteString("#" + u.fragment)
	}
	return b.String()
}
