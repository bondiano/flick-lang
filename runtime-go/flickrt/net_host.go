package flickrt

import (
	"net/netip"
	"regexp"
	"strconv"
	"strings"
)

// Effect's hosts and socket addresses (effect/net Host, NetAddress): domain
// names and scoped IPv6 literals are their canonical strings.

// inetAddress is an IP socket address; scope is an IPv6 one's zone index.
type inetAddress struct {
	addr  netip.Addr
	port  int
	scope uint32
}

type unixPathAddress struct{ path string }

// hostPort is a host (an address, a scoped literal or a domain name) and a
// port.
type hostPort struct {
	host Value
	port int
}

var (
	labelPattern = regexp.MustCompile(`^[a-z0-9_](?:[a-z0-9_-]{0,61}[a-z0-9_])?$`)
	numericLabel = regexp.MustCompile(`^(?:\d+|0x[0-9a-f]*)$`)
	domainChars  = regexp.MustCompile(`^[a-z0-9_.-]+$`)
	asciiName    = regexp.MustCompile(`(?i)^[a-z0-9_.-]$`)
	dottedDigits = regexp.MustCompile(`^[\d.]+$`)
	// JS `/[%[\]/\s\p{Cc}]/u`: Unicode whitespace too
	badZone = regexp.MustCompile(`[%\[\]/\s\p{Zs}\p{Cc}\x{2028}\x{2029}\x{feff}]`)
)

func validateDomainName(name string) string {
	if name == "." {
		return ""
	}
	relative := strings.TrimSuffix(name, ".")
	switch n := len(utf16Units(relative)); {
	case n == 0:
		return "domain name must not be empty"
	case n > 253:
		return "domain name must be at most 253 characters"
	}
	labels := strings.Split(relative, ".")
	for _, l := range labels {
		switch {
		case l == "":
			return "domain name labels must not be empty"
		case len(utf16Units(l)) > 63:
			return "domain name labels must be at most 63 characters"
		case !labelPattern.MatchString(l):
			return "domain name labels must contain only letters, digits, hyphens, or underscores and must not start or end with a hyphen"
		}
	}
	if numericLabel.MatchString(labels[len(labels)-1]) {
		return "the last domain name label must not be numeric"
	}
	return ""
}

// toAsciiDomainName lowercases an ASCII name; another goes through the
// URL host parser, as Effect's does.
func toAsciiDomainName(s string) (string, bool) {
	ascii := true
	for _, r := range s {
		if r < 0x80 {
			if !asciiName.MatchString(string(r)) {
				return "", false
			}
		} else {
			ascii = false
		}
	}
	if ascii {
		return strings.ToLower(s), true
	}
	return parseHost(s, true)
}

func isDomainName(v Value) bool {
	s, ok := v.(string)
	return ok && domainChars.MatchString(s) && validateDomainName(s) == ""
}

func domainNameFromString(s string) (Value, string) {
	name, ok := toAsciiDomainName(s)
	if !ok {
		return nil, "invalid domain name"
	}
	if msg := validateDomainName(name); msg != "" {
		return nil, msg
	}
	return name, ""
}

func scopedFromString(s string) (Value, string) {
	sep := strings.Index(s, "%")
	if sep == -1 {
		return nil, "expected an IPv6 zone"
	}
	zone := s[sep+1:]
	if zone == "" || badZone.MatchString(zone) {
		return nil, "invalid IPv6 scope identifier"
	}
	if digitsOnly.MatchString(zone) {
		n, _ := strconv.ParseFloat(zone, 64)
		if n > 0xffffffff {
			return nil, "scope identifier must be an unsigned 32-bit integer"
		}
		zone = strconv.FormatUint(uint64(n), 10)
	}
	a, msg := ipv6FromString(s[:sep])
	if msg != "" {
		return nil, msg
	}
	return formatIp(a) + "%" + zone, ""
}

func isScopedLiteral(v Value) bool {
	s, ok := v.(string)
	if !ok || !strings.Contains(s, "%") {
		return false
	}
	canonical, msg := scopedFromString(s)
	return msg == "" && canonical == s
}

func hostFromString(s string) (Value, string) {
	switch {
	case strings.Contains(s, "%"):
		return scopedFromString(s)
	case strings.Contains(s, ":"):
		return addrParse(ipFromString)(s)
	case dottedDigits.MatchString(s):
		if a, msg := ipFromString(s); msg == "" {
			return a, ""
		}
	}
	return domainNameFromString(s)
}

func formatHost(h Value) string {
	if a, ok := h.(netip.Addr); ok {
		return formatIp(a)
	}
	return h.(string)
}

// portOf is an unpadded decimal port, or why it is not one.
func portOf(text string) (int, string) {
	if !unpadded.MatchString(text) {
		return 0, "port must be an unpadded decimal integer"
	}
	n, err := strconv.Atoi(text)
	if err != nil || n > 0xffff {
		return 0, "port must be an integer from 0 through 65535"
	}
	return n, ""
}

func hostPortFromString(s string) (Value, string) {
	var hostText, portText string
	if strings.HasPrefix(s, "[") {
		end := strings.Index(s, "]")
		if end == -1 || end+1 >= len(s) || s[end+1] != ':' {
			return nil, "expected [IPv6]:port"
		}
		hostText, portText = s[1:end], s[end+2:]
		if !strings.Contains(hostText, ":") {
			return nil, "only IPv6 addresses use brackets"
		}
	} else {
		sep := strings.LastIndex(s, ":")
		if sep == -1 {
			return nil, "expected host:port"
		}
		if strings.Index(s, ":") != sep {
			return nil, "IPv6 addresses must be bracketed"
		}
		hostText, portText = s[:sep], s[sep+1:]
	}
	port, msg := portOf(portText)
	if msg != "" {
		return nil, msg
	}
	host, msg := hostFromString(hostText)
	if msg != "" {
		return nil, msg
	}
	return &hostPort{host, port}, ""
}

func formatHostPort(h *hostPort) string {
	host := formatHost(h.host)
	if strings.Contains(host, ":") {
		return "[" + host + "]:" + strconv.Itoa(h.port)
	}
	return host + ":" + strconv.Itoa(h.port)
}

func inetFromString(s string) (Value, string) {
	var host, portText string
	var scope uint32
	bracketed := strings.HasPrefix(s, "[")
	if bracketed {
		end := strings.Index(s, "]")
		if end < 0 || end+1 >= len(s) || s[end+1] != ':' || strings.Contains(s[end+1:], "]") {
			return nil, "expected [IPv6]:port"
		}
		host, portText = s[1:end], s[end+2:]
		if sep := strings.Index(host, "%"); sep != -1 {
			scopeText := host[sep+1:]
			if strings.Contains(scopeText, "%") || !digitsOnly.MatchString(scopeText) {
				return nil, "scope identifier must be decimal"
			}
			n, err := strconv.ParseUint(scopeText, 10, 32)
			if err != nil {
				return nil, "scope identifier must be an unsigned 32-bit integer"
			}
			host, scope = host[:sep], uint32(n)
		}
	} else {
		sep := strings.LastIndex(s, ":")
		if sep < 0 {
			return nil, "expected host:port or [IPv6]:port"
		}
		if strings.Index(s, ":") != sep {
			return nil, "IPv6 addresses must be bracketed"
		}
		host, portText = s[:sep], s[sep+1:]
	}
	if !unpadded.MatchString(portText) {
		return nil, "port must be an unpadded decimal integer"
	}
	addr, msg := ipFromString(host)
	if msg != "" {
		return nil, msg
	}
	if bracketed != addr.Is6() {
		return nil, "only IPv6 addresses use brackets"
	}
	port, msg := portOf(portText)
	if msg != "" {
		return nil, msg
	}
	return &inetAddress{addr, port, scope}, ""
}

func formatInet(i *inetAddress) string {
	host := formatIp(i.addr)
	if i.addr.Is4() {
		return host + ":" + strconv.Itoa(i.port)
	}
	if i.scope != 0 {
		host += "%" + strconv.FormatUint(uint64(i.scope), 10)
	}
	return "[" + host + "]:" + strconv.Itoa(i.port)
}
