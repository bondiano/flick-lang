package flickrt

import (
	"net/netip"
	"strconv"
	"strings"
)

// Hosts of URLs as the URL Standard parses them: IPv6 in brackets, IPv4
// in any of the forms browsers accept, domains in ASCII, and the opaque
// hosts of other schemes.

const forbiddenHost = "\x00\t\n\r #/:<>?@[\\]^|"

func parseHost(s string, special bool) (string, bool) {
	if strings.HasPrefix(s, "[") {
		if !strings.HasSuffix(s, "]") {
			return "", false
		}
		return parseIPv6(s[1 : len(s)-1])
	}
	if !special {
		if strings.ContainsAny(s, forbiddenHost) {
			return "", false
		}
		return percentEncode(s, c0Set), true
	}
	if s == "" {
		return "", true
	}
	domain, ok := domainToASCII(utf8Lossy(percentDecode(s)))
	if !ok {
		return "", false
	}
	for i := 0; i < len(domain); i++ {
		if c := domain[i]; c < 0x20 || c == '%' || c == 0x7F || strings.IndexByte(forbiddenHost, c) >= 0 {
			return "", false
		}
	}
	if endsInNumber(domain) {
		return parseIPv4(domain)
	}
	return domain, true
}

// domainToASCII is the domain in lower case, each label that is not ASCII
// in Punycode.
func domainToASCII(domain string) (string, bool) {
	ascii := true
	for i := 0; i < len(domain); i++ {
		ascii = ascii && domain[i] < 0x80
	}
	if ascii {
		return strings.ToLower(domain), domain != ""
	}
	// lower case and NFKC: most of what UTS 46 maps
	labels := strings.Split(runesString(compose(decomposeRunes(ToLowerCase(domain).(string), true))), ".")
	for i, label := range labels {
		for j := 0; j < len(label); j++ {
			if label[j] >= 0x80 {
				labels[i] = "xn--" + punycode([]rune(label))
				break
			}
		}
	}
	return strings.Join(labels, "."), true
}

// punycode is RFC 3492's encoding of the label.
func punycode(label []rune) string {
	const base, tMin, tMax, skew, damp = 36, 1, 26, 38, 700
	var out []byte
	for _, r := range label {
		if r < 0x80 {
			out = append(out, byte(r))
		}
	}
	basic := len(out)
	if basic > 0 {
		out = append(out, '-')
	}
	digit := func(d int) byte {
		if d < 26 {
			return byte('a' + d)
		}
		return byte('0' + d - 26)
	}
	n, delta, bias, done := 0x80, 0, 72, basic
	for done < len(label) {
		m := rune(0x7FFFFFFF)
		for _, r := range label {
			if int(r) >= n && r < m {
				m = r
			}
		}
		delta += (int(m) - n) * (done + 1)
		n = int(m)
		for _, r := range label {
			if int(r) < n {
				delta++
			}
			if int(r) != n {
				continue
			}
			q := delta
			for k := base; ; k += base {
				t := min(max(k-bias, tMin), tMax)
				if q < t {
					break
				}
				out = append(out, digit(t+(q-t)%(base-t)))
				q = (q - t) / (base - t)
			}
			out = append(out, digit(q))
			// adapt the bias to the deltas seen so far
			d := delta / damp
			if done != basic {
				d = delta / 2
			}
			d += d / (done + 1)
			k := 0
			for d > (base-tMin)*tMax/2 {
				d /= base - tMin
				k += base
			}
			bias = k + (base-tMin+1)*d/(d+skew)
			delta = 0
			done++
		}
		delta++
		n++
	}
	return string(out)
}

// endsInNumber is whether the last label, a final empty one aside, is a
// number: then the host is read as IPv4.
func endsInNumber(domain string) bool {
	labels := strings.Split(domain, ".")
	if labels[len(labels)-1] == "" && len(labels) > 1 {
		labels = labels[:len(labels)-1]
	}
	last := labels[len(labels)-1]
	if last == "" {
		return false
	}
	if strings.Trim(last, "0123456789") == "" {
		return true
	}
	return (strings.HasPrefix(last, "0x") || strings.HasPrefix(last, "0X")) &&
		strings.Trim(last[2:], "0123456789abcdefABCDEF") == ""
}

func parseIPv4Number(s string) (uint64, bool) {
	radix := 10
	switch {
	case strings.HasPrefix(s, "0x") || strings.HasPrefix(s, "0X"):
		s, radix = s[2:], 16
	case len(s) > 1 && s[0] == '0':
		s, radix = s[1:], 8
	}
	if s == "" {
		return 0, true
	}
	n, err := strconv.ParseUint(s, radix, 64)
	return n, err == nil && s[0] != '+'
}

func parseIPv4(s string) (string, bool) {
	parts := strings.Split(s, ".")
	if parts[len(parts)-1] == "" && len(parts) > 1 {
		parts = parts[:len(parts)-1]
	}
	if len(parts) > 4 {
		return "", false
	}
	var ip uint64
	for i, p := range parts {
		if p == "" {
			return "", false
		}
		n, ok := parseIPv4Number(p)
		if !ok {
			return "", false
		}
		if i < len(parts)-1 {
			if n > 255 {
				return "", false
			}
			ip += n << (8 * (3 - i))
		} else {
			if n >= 1<<(8*(5-len(parts))) {
				return "", false
			}
			ip += n
		}
	}
	return strconv.Itoa(int(ip>>24)) + "." + strconv.Itoa(int(ip>>16&255)) + "." +
		strconv.Itoa(int(ip>>8&255)) + "." + strconv.Itoa(int(ip&255)), true
}

// parseIPv6 is the bracketed text of an IPv6 address: its pieces in hex,
// the first longest run of zero pieces compressed.
func parseIPv6(s string) (string, bool) {
	addr, err := netip.ParseAddr(s)
	if err != nil || !addr.Is6() || addr.Zone() != "" {
		return "", false
	}
	b := addr.As16()
	var pieces [8]int
	for i := range pieces {
		pieces[i] = int(b[2*i])<<8 | int(b[2*i+1])
	}
	start, length := -1, 1
	for i := 0; i < 8; {
		j := i
		for j < 8 && pieces[j] == 0 {
			j++
		}
		if j-i > length {
			start, length = i, j-i
		}
		i = j + 1
	}
	var out strings.Builder
	out.WriteByte('[')
	for i := 0; i < 8; i++ {
		if i == start {
			out.WriteString("::")
			i += length - 1
			continue
		}
		if i > 0 && i != start+length {
			out.WriteByte(':')
		}
		out.WriteString(strconv.FormatInt(int64(pieces[i]), 16))
	}
	out.WriteByte(']')
	return out.String(), true
}
