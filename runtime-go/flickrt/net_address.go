package flickrt

import (
	"net/netip"
	"regexp"
	"strconv"
	"strings"
)

// Network addresses (NetAddress, IpInterface, IpNetwork): parsed from text
// with the rules and messages of the JS target, and written back in
// canonical form. An IP address is a netip.Addr (an IPv6 one never
// unmapped), a MAC address a macAddress.

type macAddress [6]byte

// ipInterface is an address with a prefix length; network is whether it is
// an IpNetwork, whose host bits are zero.
type ipInterface struct {
	addr    netip.Addr
	prefix  int
	network bool
}

var (
	ipv4Part   = regexp.MustCompile(`^\d{1,3}$`)
	ipv6Part   = regexp.MustCompile(`^[0-9a-fA-F]{1,4}$`)
	macText    = regexp.MustCompile(`^(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}$`)
	unpadded   = regexp.MustCompile(`^(0|[1-9][0-9]*)$`)
	digitsOnly = regexp.MustCompile(`^\d+$`)
)

// An address parser: the value, or the message of a NetAddressError.
type netParse func(string) (Value, string)

func ipv4FromString(s string) (netip.Addr, string) {
	parts := strings.Split(s, ".")
	if len(parts) != 4 {
		return netip.Addr{}, "expected exactly four decimal octets"
	}
	for _, p := range parts {
		if !ipv4Part.MatchString(p) {
			return netip.Addr{}, "expected exactly four decimal octets"
		}
	}
	var b [4]byte
	for _, p := range parts {
		if len(p) > 1 && p[0] == '0' {
			return netip.Addr{}, "leading zeroes are not allowed"
		}
	}
	for i, p := range parts {
		n, _ := strconv.Atoi(p)
		if n > 255 {
			return netip.Addr{}, "octets must be at most 255"
		}
		b[i] = byte(n)
	}
	return netip.AddrFrom4(b), ""
}

func ipv6FromString(s string) (netip.Addr, string) {
	fail := func(m string) (netip.Addr, string) { return netip.Addr{}, m }
	if strings.ContainsAny(s, "[]%") {
		return fail("brackets and zone identifiers are not valid in a bare IPv6 address")
	}
	halves := strings.Split(s, "::")
	if len(halves) > 2 {
		return fail("only one compression marker is allowed")
	}
	var head, tail []string
	if halves[0] != "" {
		head = strings.Split(halves[0], ":")
	}
	if len(halves) == 2 && halves[1] != "" {
		tail = strings.Split(halves[1], ":")
	}
	for _, p := range append(append([]string{}, head...), tail...) {
		if p == "" {
			return fail("empty segments are only valid in the compression marker")
		}
	}
	trailing := ""
	if len(tail) > 0 {
		trailing = tail[len(tail)-1]
	} else if len(head) > 0 {
		trailing = head[len(head)-1]
	}
	var embedded []uint16
	if strings.Contains(trailing, ".") {
		if len(halves) == 2 && len(tail) == 0 {
			return fail("embedded IPv4 syntax must be trailing")
		}
		v4, msg := ipv4FromString(trailing)
		if msg != "" {
			return fail(msg)
		}
		b := v4.As4()
		embedded = []uint16{uint16(b[0])<<8 | uint16(b[1]), uint16(b[2])<<8 | uint16(b[3])}
		if len(tail) > 0 {
			tail = tail[:len(tail)-1]
		} else {
			head = head[:len(head)-1]
		}
	}
	explicit := len(head) + len(tail) + len(embedded)
	if len(halves) == 1 && explicit != 8 {
		return fail("expected eight segments")
	}
	if len(halves) == 2 && explicit >= 8 {
		return fail("compression must replace at least one segment")
	}
	segments := make([]uint16, 0, 8)
	for i, p := range append(append(head, make([]string, 8-explicit)...), tail...) {
		if i >= len(head) && i < len(head)+8-explicit {
			segments = append(segments, 0)
			continue
		}
		if !ipv6Part.MatchString(p) {
			return fail("segments must contain one through four hexadecimal digits")
		}
		n, _ := strconv.ParseUint(p, 16, 16)
		segments = append(segments, uint16(n))
	}
	segments = append(segments, embedded...)
	var b [16]byte
	for i, seg := range segments {
		b[2*i], b[2*i+1] = byte(seg>>8), byte(seg)
	}
	return netip.AddrFrom16(b), ""
}

func ipFromString(s string) (netip.Addr, string) {
	if strings.Contains(s, ":") {
		return ipv6FromString(s)
	}
	return ipv4FromString(s)
}

func macFromString(s string) (Value, string) {
	if !macText.MatchString(s) {
		return nil, "expected six two-digit hexadecimal octets separated by colons"
	}
	var m macAddress
	for i, p := range strings.Split(s, ":") {
		n, _ := strconv.ParseUint(p, 16, 8)
		m[i] = byte(n)
	}
	return m, ""
}

func formatMac(m macAddress) string {
	parts := make([]string, 6)
	for i, b := range m {
		parts[i] = strconv.FormatUint(uint64(b)|0x100, 16)[1:]
	}
	return strings.Join(parts, ":")
}

// formatIp is the canonical text of an address: an IPv4-mapped address in
// dotted form, otherwise the longest run of two or more zero segments
// compressed.
func formatIp(a netip.Addr) string {
	if a.Is4() {
		return a.String()
	}
	b := a.As16()
	var seg [8]uint16
	for i := range seg {
		seg[i] = uint16(b[2*i])<<8 | uint16(b[2*i+1])
	}
	if a.Is4In6() {
		return "::ffff:" + netip.AddrFrom4([4]byte(b[12:])).String()
	}
	bestStart, bestLength, start := -1, 0, -1
	for i := 0; i <= 8; i++ {
		if i < 8 && seg[i] == 0 {
			if start == -1 {
				start = i
			}
		} else if start != -1 {
			if i-start > bestLength {
				bestStart, bestLength = start, i-start
			}
			start = -1
		}
	}
	hex := func(xs []uint16) string {
		parts := make([]string, len(xs))
		for i, x := range xs {
			parts[i] = strconv.FormatUint(uint64(x), 16)
		}
		return strings.Join(parts, ":")
	}
	if bestLength < 2 {
		return hex(seg[:])
	}
	return hex(seg[:bestStart]) + "::" + hex(seg[bestStart+bestLength:])
}

// The predicates of addresses.
func ipv4Value(a netip.Addr) uint32 {
	b := a.As4()
	return uint32(b[0])<<24 | uint32(b[1])<<16 | uint32(b[2])<<8 | uint32(b[3])
}

func isUnspecified(a netip.Addr) bool { return a.IsUnspecified() }
func isLoopback(a netip.Addr) bool {
	if a.Is4() {
		return a.As4()[0] == 127
	}
	return a == netip.IPv6Loopback()
}
func isMulticast(a netip.Addr) bool {
	if a.Is4() {
		return a.As4()[0]>>4 == 0xe
	}
	return a.As16()[0] == 0xff
}
func isBroadcast(a netip.Addr) bool { return a.Is4() && ipv4Value(a) == 0xffffffff }
func isUnicast(a netip.Addr) bool   { return !isMulticast(a) && !isUnspecified(a) && !isBroadcast(a) }
func isLinkLocal(a netip.Addr) bool {
	if a.Is4() {
		return ipv4Value(a)>>16 == 0xa9fe
	}
	b := a.As16()
	return b[0] == 0xfe && b[1]&0xc0 == 0x80
}
func isPrivate(a netip.Addr) bool {
	v := ipv4Value(a)
	return v>>24 == 10 || (v>>16)&0xfff0 == 0xac10 || v>>16 == 0xc0a8
}
func isUniqueLocal(a netip.Addr) bool { return a.As16()[0]&0xfe == 0xfc }

// parseWithPrefix reads `address/prefix`; without a slash the prefix is
// the address's width unless it is required.
func parseWithPrefix(s string, required bool, parse func(string) (netip.Addr, string)) (*ipInterface, string) {
	slash := strings.Index(s, "/")
	text, prefix := s, -1
	if slash != -1 || required {
		if slash <= 0 || slash != strings.LastIndex(s, "/") || slash == len(s)-1 {
			return nil, "expected an address and prefix length separated by one slash"
		}
		if !unpadded.MatchString(s[slash+1:]) {
			return nil, "prefix length must be an unpadded ASCII decimal integer"
		}
		text = s[:slash]
		n, err := strconv.Atoi(s[slash+1:])
		if err != nil {
			n = 1 << 30
		}
		prefix = n
	}
	addr, msg := parse(text)
	if msg != "" {
		return nil, msg
	}
	if prefix == -1 {
		prefix = addr.BitLen()
	}
	if prefix > addr.BitLen() {
		return nil, "prefix length must be an integer from 0 through " + strconv.Itoa(addr.BitLen())
	}
	return &ipInterface{addr, prefix, false}, ""
}

func interfaceParse(parse func(string) (netip.Addr, string)) netParse {
	return func(s string) (Value, string) {
		i, msg := parseWithPrefix(s, false, parse)
		if msg != "" {
			return nil, msg
		}
		return i, ""
	}
}

func networkParse(parse func(string) (netip.Addr, string)) netParse {
	return func(s string) (Value, string) {
		i, msg := parseWithPrefix(s, true, parse)
		if msg != "" {
			return nil, msg
		}
		if netip.PrefixFrom(i.addr, i.prefix).Masked().Addr() != i.addr {
			return nil, "address has non-zero host bits"
		}
		return &ipInterface{i.addr, i.prefix, true}, ""
	}
}

func formatInterface(i *ipInterface) string { return formatIp(i.addr) + "/" + strconv.Itoa(i.prefix) }

// addrParse is an IP parser as an address parser.
func addrParse(parse func(string) (netip.Addr, string)) netParse {
	return func(s string) (Value, string) {
		a, msg := parse(s)
		if msg != "" {
			return nil, msg
		}
		return a, ""
	}
}
