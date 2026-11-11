package flickrt

import (
	"encoding/hex"
	"net/netip"
	"strconv"
	"strings"
)

// DNS records and their JSON codecs: a record is its JSON object, `_tag`
// first, decoded by the record type's fields.
//
// shortcut: SOA timers stay whole seconds, where the JS target holds Durations; nothing reads them but format_record and the codec, which write seconds alike
type dnsRecord struct{ fields *Record }

func (r *dnsRecord) tag() string { return r.fields.Get("_tag").(string) }

var dnsRecordTypes = []string{"A", "AAAA", "CAA", "CNAME", "MX", "NAPTR", "NS", "PTR", "SOA", "SRV", "TLSA", "TXT"}

// dnsAddress is the canonical address of a record: text that parses and is
// already in canonical form.
func dnsAddress(parse func(string) (netip.Addr, string), is func(Value) bool) codec {
	return netCodec("IpAddress", func(s string) (Value, string) {
		a, msg := parse(s)
		switch {
		case msg != "":
			return nil, msg
		case formatIp(a) != s:
			return nil, "expected a canonical address"
		}
		return a, ""
	}, is, formatIpValue)
}

var (
	dnsDomain   codec = withChecks(CodecString.(codec), &check{test: func(_ *Fiber, v Value) bool { return isDomainName(v) }, expected: "a normalized domain name"})
	dnsNonEmpty codec = withChecks(CodecString.(codec), isMinLength(1))
	dnsUint8    codec = withChecks(numberType, isInt(), isBetween(0, 0xff, ""))
	dnsUint16   codec = withChecks(numberType, isInt(), isBetween(0, 0xffff, ""))
	dnsUint32   codec = withChecks(numberType, isInt(), isBetween(0, 0xffffffff, ""))
	dnsCaaTag   codec = withChecks(CodecString.(codec), func() *check {
		c := *isPattern(`(?i)^[a-z0-9]+$`)
		c.expected = "a string matching the RegExp ^[a-z0-9]+$"
		return &c
	}())
	dnsHexBytes codec = &stage{
		&stringAs{"a hex encoded string that will be decoded as a Uint8Array"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if b, ok := decodeHex(v.(string)); ok {
				return b, nil
			}
			return nil, fail("an invalid value")
		},
		pure(func(v Value) Value { return hex.EncodeToString(v.([]byte)) }),
	}
	stringCodec = CodecString.(codec)

	// the fields of each record type, after its `_tag`
	dnsFields = map[string][]any{
		"A":     {"address", dnsAddress(ipv4FromString, isIpv4)},
		"AAAA":  {"address", dnsAddress(ipv6FromString, isIpv6)},
		"CAA":   {"critical", CodecBool, "tag", dnsCaaTag, "value", stringCodec},
		"CNAME": {"target", dnsDomain},
		"MX":    {"exchange", dnsDomain, "priority", dnsUint16},
		"NAPTR": {"order", dnsUint16, "preference", dnsUint16, "flags", stringCodec, "service", stringCodec,
			"regexp", stringCodec, "replacement", dnsDomain},
		"NS":  {"host", dnsDomain},
		"PTR": {"host", dnsNonEmpty},
		"SOA": {"primary", dnsDomain, "admin", dnsNonEmpty, "serial", dnsUint32, "refresh", dnsUint32,
			"retry", dnsUint32, "expire", dnsUint32, "minimum", dnsUint32},
		"SRV":  {"target", dnsDomain, "port", dnsUint16, "priority", dnsUint16, "weight", dnsUint16},
		"TLSA": {"certUsage", dnsUint8, "selector", dnsUint8, "matchingType", dnsUint8, "data", dnsHexBytes},
		"TXT":  {"chunks", CodecNonEmptyList(CodecString)},
	}

	dnsCodecs = func() map[string]codec {
		out := map[string]codec{}
		for _, t := range dnsRecordTypes {
			out[t] = dnsRecordCodec(t)
		}
		return out
	}()
)

// dnsRecordCodec is the JSON codec of the records of type `t`: the block
// of its `_tag` and fields, then the record made of them.
func dnsRecordCodec(t string) codec {
	entries := []Value{"_tag", DslField("_tag", CodecLiteral(t))}
	fields := dnsFields[t]
	for i := 0; i < len(fields); i += 2 {
		entries = append(entries, fields[i], DslField(fields[i], fields[i+1]))
	}
	name := "Dns" + t[:1] + strings.ToLower(t[1:]) + "Record"
	return &stage{
		CodecBlock(name, entries).(codec),
		func(_ *Fiber, v Value) (Value, []issue) {
			r := record(v)
			if t == "TLSA" && len(r.Get("data").([]byte)) == 0 {
				return nil, fail("invalid TLSA record data")
			}
			return &dnsRecord{r}, nil
		},
		typed(name, func(v Value) bool { r, ok := v.(*dnsRecord); return ok && r.tag() == t },
			func(v Value) Value { return v.(*dnsRecord).fields }),
	}
}

// dnsUnion is the union of the record types: the member of the input's
// `_tag`, or of the record's type. Input of no known `_tag` is the union's
// issue, as no member is a candidate for it.
type dnsUnion struct{}

func (dnsUnion) accepts(in Value) bool { _, ok := in.(*Record); return ok }

func (dnsUnion) decode(fb *Fiber, in Value) (Value, []issue) {
	if r, ok := in.(*Record); ok {
		if t, ok := recordField(r, "_tag").(string); ok && dnsCodecs[t] != nil {
			return dnsCodecs[t].decode(fb, in)
		}
	}
	return nil, expected("Union", in)
}

func (dnsUnion) encode(fb *Fiber, v Value) (Value, []issue) {
	if r, ok := v.(*dnsRecord); ok {
		return dnsCodecs[r.tag()].encode(fb, v)
	}
	return nil, expected("Union", v)
}

func dnsJson(t string) Value { return &fallible{dnsCodecs[t]} }

var (
	CodecDnsRecord      Value = &fallible{dnsUnion{}}
	CodecDnsARecord           = dnsJson("A")
	CodecDnsAaaaRecord        = dnsJson("AAAA")
	CodecDnsCaaRecord         = dnsJson("CAA")
	CodecDnsCnameRecord       = dnsJson("CNAME")
	CodecDnsMxRecord          = dnsJson("MX")
	CodecDnsNaptrRecord       = dnsJson("NAPTR")
	CodecDnsNsRecord          = dnsJson("NS")
	CodecDnsPtrRecord         = dnsJson("PTR")
	CodecDnsSoaRecord         = dnsJson("SOA")
	CodecDnsSrvRecord         = dnsJson("SRV")
	CodecDnsTlsaRecord        = dnsJson("TLSA")
	CodecDnsTxtRecord         = dnsJson("TXT")
	CodecDnsRecordType        = CodecLiterals(func() []Value {
		out := make([]Value, len(dnsRecordTypes))
		for i, t := range dnsRecordTypes {
			out[i] = t
		}
		return out
	}())
)

// quoteDns is a character string in DNS presentation format (RFC 1035
// 5.1): `"` and `\` escaped, bytes outside printable ASCII as `\DDD`.
func quoteDns(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, c := range []byte(wellFormed(s)) {
		switch {
		case c == '"' || c == '\\':
			b.WriteByte('\\')
			b.WriteByte(c)
		case c < 0x20 || c > 0x7e:
			b.WriteString(decimalEscape(c))
		default:
			b.WriteByte(c)
		}
	}
	b.WriteByte('"')
	return b.String()
}

func decimalEscape(c byte) string { return "\\" + strconv.FormatUint(uint64(c)+1000, 10)[1:] }

// formatDnsName is a name held as text in presentation format: its `\.`
// and `\\` kept, other specials escaped, spaces and other bytes as `\DDD`.
func formatDnsName(s string) string {
	bs := []byte(wellFormed(s))
	var b strings.Builder
	for i := 0; i < len(bs); i++ {
		c := bs[i]
		switch {
		case c == '\\' && i+1 < len(bs) && (bs[i+1] == '.' || bs[i+1] == '\\'):
			b.WriteByte('\\')
			b.WriteByte(bs[i+1])
			i++
		case c == '\\':
			b.WriteString(`\\`)
		case c <= 0x20 || c > 0x7e:
			b.WriteString(decimalEscape(c))
		case strings.IndexByte(`"$();@`, c) >= 0:
			b.WriteByte('\\')
			b.WriteByte(c)
		default:
			b.WriteByte(c)
		}
	}
	return b.String()
}

// NetFormatRecord is a record as one line of zone file.
func NetFormatRecord(v Value) Value {
	r := v.(*dnsRecord)
	get := func(k string) Value { return r.fields.Get(k) }
	text := func(k string) string { return get(k).(string) }
	num := func(k string) string { return jsString(get(k)) }
	switch t := r.tag(); t {
	case "A", "AAAA":
		return t + " " + formatIp(addrOf(get("address")))
	case "CAA":
		flag := "0"
		if get("critical").(bool) {
			flag = "128"
		}
		return "CAA " + flag + " " + text("tag") + " " + quoteDns(text("value"))
	case "CNAME":
		return "CNAME " + text("target")
	case "MX":
		return "MX " + num("priority") + " " + text("exchange")
	case "NAPTR":
		return "NAPTR " + num("order") + " " + num("preference") + " " + quoteDns(text("flags")) + " " +
			quoteDns(text("service")) + " " + quoteDns(text("regexp")) + " " + text("replacement")
	case "NS":
		return "NS " + text("host")
	case "PTR":
		return "PTR " + formatDnsName(text("host"))
	case "SOA":
		return "SOA " + text("primary") + " " + formatDnsName(text("admin")) + " " + num("serial") + " " +
			num("refresh") + " " + num("retry") + " " + num("expire") + " " + num("minimum")
	case "SRV":
		return "SRV " + num("priority") + " " + num("weight") + " " + num("port") + " " + text("target")
	case "TLSA":
		return "TLSA " + num("certUsage") + " " + num("selector") + " " + num("matchingType") + " " +
			strings.ToUpper(hex.EncodeToString(get("data").([]byte)))
	}
	chunks := List(get("chunks"))
	parts := make([]string, len(chunks))
	for i, c := range chunks {
		parts[i] = quoteDns(c.(string))
	}
	return "TXT " + strings.Join(parts, " ")
}
