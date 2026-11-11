package flickrt

import "net/netip"

// The JSON codecs of network addresses: text decoded by an address parser,
// whose failure message is the issue, written back in canonical form;
// refinements are checks of the decoded address, and a host or socket
// address is a union of codecs.

// netCodec is the codec of an address from its text: `is` is the declared
// type's guard, which encoding checks first.
func netCodec(name string, parse netParse, is func(Value) bool, format func(Value) string) codec {
	return &stage{
		CodecString.(codec),
		func(_ *Fiber, v Value) (Value, []issue) {
			x, msg := parse(v.(string))
			if msg != "" {
				return nil, fail(msg)
			}
			return x, nil
		},
		typed(name, is, func(v Value) Value { return format(v) }),
	}
}

// netRefine is a refinement of an address codec: what it decodes passes
// `test`, else "expected <expected>".
func netRefine(c codec, expected string, test func(Value) bool) codec {
	return withChecks(c, &check{test: func(_ *Fiber, v Value) bool { return test(v) }, expected: expected})
}

// netUnion is a union of string codecs: the first member that decodes, or
// encodes, the value, else every member's issues. Input no member takes is
// the union's issue, as no member is a candidate for it.
type netUnion []codec

func (u netUnion) accepts(in Value) bool { _, ok := in.(string); return ok }

func (u netUnion) decode(fb *Fiber, in Value) (Value, []issue) {
	if !u.accepts(in) {
		return nil, expected("Union", in)
	}
	return u.first(fb, in, codec.decode)
}

func (u netUnion) encode(fb *Fiber, v Value) (Value, []issue) { return u.first(fb, v, codec.encode) }

func (u netUnion) first(fb *Fiber, in Value, run func(codec, *Fiber, Value) (Value, []issue)) (Value, []issue) {
	var all []issue
	for _, m := range u {
		v, issues := run(m, fb, in)
		if issues == nil {
			return v, nil
		}
		all = append(all, issues...)
	}
	return nil, all
}

func isIp(v Value) bool   { _, ok := v.(netip.Addr); return ok }
func isIpv4(v Value) bool { a, ok := v.(netip.Addr); return ok && a.Is4() }
func isIpv6(v Value) bool { a, ok := v.(netip.Addr); return ok && a.Is6() }
func isMac(v Value) bool  { _, ok := v.(macAddress); return ok }

// isPrefixed is the guard of an interface (network false) or a network of
// the addresses `family` accepts.
func isPrefixed(network bool, family func(Value) bool) func(Value) bool {
	return func(v Value) bool { i, ok := v.(*ipInterface); return ok && i.network == network && family(i.addr) }
}

func isInet(family func(Value) bool) func(Value) bool {
	return func(v Value) bool { i, ok := v.(*inetAddress); return ok && family(i.addr) }
}

func addrOf(v Value) netip.Addr { return v.(netip.Addr) }

func formatIpValue(v Value) string        { return formatIp(addrOf(v)) }
func formatMacValue(v Value) string       { return formatMac(v.(macAddress)) }
func formatInterfaceValue(v Value) string { return formatInterface(v.(*ipInterface)) }
func formatInetValue(v Value) string      { return formatInet(v.(*inetAddress)) }
func formatText(v Value) string           { return v.(string) }

var (
	ipCodec   = netCodec("IpAddress", addrParse(ipFromString), isIp, formatIpValue)
	ipv4Codec = netCodec("Ipv4Address", addrParse(ipv4FromString), isIpv4, formatIpValue)
	ipv6Codec = netCodec("Ipv6Address", addrParse(ipv6FromString), isIpv6, formatIpValue)
	macCodec  = netCodec("MacAddress", macFromString, isMac, formatMacValue)
	inetCodec = netCodec("InetAddress", inetFromString, isInet(isIp), formatInetValue)
	unixCodec = netCodec("UnixPathAddress",
		func(s string) (Value, string) { return &unixPathAddress{s}, "" },
		func(v Value) bool { _, ok := v.(*unixPathAddress); return ok },
		func(v Value) string { return v.(*unixPathAddress).path })
	domainCodec = netCodec("DomainName", domainNameFromString, isDomainName, formatText)
	scopedCodec = netCodec("ScopedIpv6Literal", scopedFromString, isScopedLiteral, formatText)

	// the predicates of the refinements, over the decoded address
	onAddr = func(p func(netip.Addr) bool) func(Value) bool { return func(v Value) bool { return p(addrOf(v)) } }
	onMac  = func(p func(macAddress) bool) func(Value) bool {
		return func(v Value) bool { return p(v.(macAddress)) }
	}
)

func netJson(c codec) Value { return &fallible{c} }

var (
	CodecDomainName = netJson(domainCodec)
	CodecHost       = netJson(netUnion{ipCodec, scopedCodec, domainCodec})
	CodecHostPort   = netJson(netCodec("HostPort", hostPortFromString,
		func(v Value) bool { _, ok := v.(*hostPort); return ok },
		func(v Value) string { return formatHostPort(v.(*hostPort)) }))
	CodecInetAddress   = netJson(inetCodec)
	CodecInetAddressV4 = netJson(netRefine(inetCodec, "an IPv4 socket address", isInet(isIpv4)))
	CodecInetAddressV6 = netJson(netRefine(inetCodec, "an IPv6 socket address", isInet(isIpv6)))
	CodecSocketAddress = netJson(netUnion{inetCodec, unixCodec})
	CodecUnixPath      = netJson(unixCodec)

	CodecIpAddress   = netJson(ipCodec)
	CodecIpInterface = netJson(netCodec("IpInterface", interfaceParse(ipFromString), isPrefixed(false, isIp), formatInterfaceValue))
	CodecIpNetwork   = netJson(netCodec("IpNetwork", networkParse(ipFromString), isPrefixed(true, isIp), formatInterfaceValue))

	CodecIpLinkLocal   = netJson(netRefine(ipCodec, "a link-local address", onAddr(isLinkLocal)))
	CodecIpLoopback    = netJson(netRefine(ipCodec, "a loopback address", onAddr(isLoopback)))
	CodecIpMulticast   = netJson(netRefine(ipCodec, "a multicast address", onAddr(isMulticast)))
	CodecIpUnicast     = netJson(netRefine(ipCodec, "a unicast address", onAddr(isUnicast)))
	CodecIpUnspecified = netJson(netRefine(ipCodec, "an unspecified address", onAddr(isUnspecified)))

	CodecIpv4Address   = netJson(ipv4Codec)
	CodecIpv4Broadcast = netJson(netRefine(ipv4Codec, "a broadcast address", onAddr(isBroadcast)))
	CodecIpv4Private   = netJson(netRefine(ipv4Codec, "a private address", onAddr(isPrivate)))
	CodecIpv4Interface = netJson(netCodec("Ipv4Interface", interfaceParse(ipv4FromString), isPrefixed(false, isIpv4), formatInterfaceValue))
	CodecIpv4Network   = netJson(netCodec("Ipv4Network", networkParse(ipv4FromString), isPrefixed(true, isIpv4), formatInterfaceValue))

	CodecIpv6Address     = netJson(ipv6Codec)
	CodecIpv6UniqueLocal = netJson(netRefine(ipv6Codec, "a unique-local address", onAddr(isUniqueLocal)))
	CodecIpv6Interface   = netJson(netCodec("Ipv6Interface", interfaceParse(ipv6FromString), isPrefixed(false, isIpv6), formatInterfaceValue))
	CodecIpv6Network     = netJson(netCodec("Ipv6Network", networkParse(ipv6FromString), isPrefixed(true, isIpv6), formatInterfaceValue))

	CodecMacAddress   = netJson(macCodec)
	CodecMacBroadcast = netJson(netRefine(macCodec, "a broadcast address",
		onMac(func(m macAddress) bool { return m == macAddress{0xff, 0xff, 0xff, 0xff, 0xff, 0xff} })))
	CodecMacLocallyAdministered = netJson(netRefine(macCodec, "a locally administered address",
		onMac(func(m macAddress) bool { return m[0]&2 != 0 })))
	CodecMacMulticast               = netJson(netRefine(macCodec, "a multicast address", onMac(func(m macAddress) bool { return m[0]&1 != 0 })))
	CodecMacUnicast                 = netJson(netRefine(macCodec, "a unicast address", onMac(func(m macAddress) bool { return m[0]&1 == 0 })))
	CodecMacUniversallyAdministered = netJson(netRefine(macCodec, "a universally administered address",
		onMac(func(m macAddress) bool { return m[0]&2 == 0 })))

	CodecScopedIpv6Literal = netJson(scopedCodec)
)

// The `Net.format_*`: the canonical text of an address.
func NetFormatIp(v Value) Value        { return formatIpValue(v) }
func NetFormatInterface(v Value) Value { return formatInterfaceValue(v) }
func NetFormatMac(v Value) Value       { return formatMacValue(v) }
func NetFormatHostPort(v Value) Value  { return formatHostPort(v.(*hostPort)) }
func NetFormatInet(v Value) Value      { return formatInetValue(v) }
func NetFormatText(v Value) Value      { return v }
func NetFormatHost(v Value) Value      { return formatHost(v) }
func NetFormatUnixPath(v Value) Value  { return v.(*unixPathAddress).path }
func NetFormatSocket(v Value) Value {
	if u, ok := v.(*unixPathAddress); ok {
		return u.path
	}
	return formatInetValue(v)
}

// netParser is a `Net.parse_*`: the text decoded by the codec.
func netParser(c Value) func(*Fiber, Value) Value {
	return func(fb *Fiber, text Value) Value { return CodecDecodeResult(fb, c, text) }
}

func NetParseIp(fb *Fiber, s Value) Value        { return netParser(CodecIpAddress)(fb, s) }
func NetParseInterface(fb *Fiber, s Value) Value { return netParser(CodecIpInterface)(fb, s) }
func NetParseNetwork(fb *Fiber, s Value) Value   { return netParser(CodecIpNetwork)(fb, s) }
func NetParseMac(fb *Fiber, s Value) Value       { return netParser(CodecMacAddress)(fb, s) }
func NetParseDomain(fb *Fiber, s Value) Value    { return netParser(CodecDomainName)(fb, s) }
func NetParseScoped(fb *Fiber, s Value) Value    { return netParser(CodecScopedIpv6Literal)(fb, s) }
func NetParseHost(fb *Fiber, s Value) Value      { return netParser(CodecHost)(fb, s) }
func NetParseHostPort(fb *Fiber, s Value) Value  { return netParser(CodecHostPort)(fb, s) }
func NetParseInet(fb *Fiber, s Value) Value      { return netParser(CodecInetAddress)(fb, s) }
func NetParseUnixPath(fb *Fiber, s Value) Value  { return netParser(CodecUnixPath)(fb, s) }
func NetParseSocket(fb *Fiber, s Value) Value    { return netParser(CodecSocketAddress)(fb, s) }
