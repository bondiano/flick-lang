package flickrt

import "testing"

func TestNetAddresses(t *testing.T) {
	for _, c := range []struct {
		codec    Value
		in, want string
	}{
		{CodecIpAddress, "2001:DB8:0:0:1:0:0:1", "2001:db8::1:0:0:1"},
		{CodecIpAddress, "::ffff:192.0.2.1", "::ffff:192.0.2.1"},
		{CodecIpAddress, "1:0:2:0:3:0:4:0", "1:0:2:0:3:0:4:0"},
		{CodecIpv6Address, "::1.2.3.4", "::102:304"},
		{CodecIpInterface, "10.1.2.3", "10.1.2.3/32"},
		{CodecHost, "Bücher.example", "xn--bcher-kva.example"},
		{CodecInetAddress, "[fe80::1%7]:443", "[fe80::1%7]:443"},
		{CodecScopedIpv6Literal, "FE80::1%007", "fe80::1%7"},
		{CodecSocketAddress, "127.0.0.1:0", "127.0.0.1:0"},
	} {
		v, issues := codecOf(c.codec).decode(rootFiber(), c.in)
		if issues != nil {
			t.Fatalf("%s gave %v", c.in, issues)
		}
		if w, _ := codecOf(c.codec).encode(rootFiber(), v); w != c.want {
			t.Errorf("%s came back as %v, want %s", c.in, w, c.want)
		}
	}
	for _, c := range []struct {
		codec    Value
		in, want string
	}{
		{CodecIpv6Address, "1::2::3", "only one compression marker is allowed"},
		{CodecIpv6Address, "1:2:3:4:5:6:7::8", "compression must replace at least one segment"},
		{CodecIpv6Address, "1.2.3.4::", "embedded IPv4 syntax must be trailing"},
		{CodecIpv4Network, "10.0.0.1/8", "address has non-zero host bits"},
		{CodecIpv4Interface, "10.0.0.1/33", "prefix length must be an integer from 0 through 32"},
		{CodecInetAddress, "[127.0.0.1]:80", "only IPv6 addresses use brackets"},
		{CodecInetAddress, "::1:80", "IPv6 addresses must be bracketed"},
		{CodecHostPort, "example.com:080", "port must be an unpadded decimal integer"},
		{CodecDomainName, "-a.com", "domain name labels must contain only letters, digits, hyphens, or underscores and must not start or end with a hyphen"},
		{CodecDomainName, "a.123", "the last domain name label must not be numeric"},
	} {
		if _, issues := codecOf(c.codec).decode(rootFiber(), c.in); len(issues) != 1 || issues[0].message != c.want {
			t.Errorf("%s gave %v, want %s", c.in, issues, c.want)
		}
	}
}

func TestDnsRecords(t *testing.T) {
	fb := rootFiber()
	decode := func(text string) Value {
		in, _ := parseJSON(text)
		v, issues := codecOf(CodecDnsRecord).decode(fb, in)
		if issues != nil {
			t.Fatalf("%s gave %v", text, issues)
		}
		return v
	}
	for text, want := range map[string]string{
		`{"_tag":"TXT","chunks":["a \"b\"","é"]}`:                                 `TXT "a \"b\"" "\195\169"`,
		`{"_tag":"PTR","host":"a b.example"}`:                                     `PTR a\032b.example`,
		`{"_tag":"TLSA","certUsage":3,"selector":1,"matchingType":1,"data":"0a"}`: `TLSA 3 1 1 0A`,
		`{"_tag":"CAA","critical":true,"tag":"issue","value":"ca"}`:               `CAA 128 issue "ca"`,
	} {
		if got := NetFormatRecord(decode(text)); got != want {
			t.Errorf("%s formatted %v, want %s", text, got, want)
		}
	}
	in, _ := parseJSON(`{"_tag":"A","address":"010.0.0.1"}`)
	if _, issues := codecOf(CodecDnsARecord).decode(fb, in); len(issues) != 1 || issues[0].message != "leading zeroes are not allowed" {
		t.Fatalf("a padded address gave %v", issues)
	}
}
