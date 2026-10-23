// Effect's network schemas for the bindings of the standard library (`Net`):
// an address codec decodes text to Effect's address value, a DNS record
// codec to Effect's record; both are opaque to Flick.
import { Schema } from "effect";
import { Dns } from "effect/net";

import type { AnySchema } from "../schema/schemas.ts";
import { fallible } from "./Schema.ts";

/** The canonical text of an address, by the codec it was decoded with. */
const format = <A>(codec: Schema.Codec<A, string>): ((a: A) => string) => Schema.encodeSync(codec);

const socket = Schema.Union([Schema.InetAddressFromString, Schema.UnixPathAddressFromString]);

export const flickFormatIp = format(Schema.IpAddressFromString);
export const flickFormatInterface = format(Schema.IpInterfaceFromString);
export const flickFormatNetwork = format(Schema.IpNetworkFromString);
export const flickFormatMac = format(Schema.MacAddressFromString);
export const flickFormatDomain = format(Schema.DomainNameFromString);
export const flickFormatScoped = format(Schema.ScopedIpv6LiteralFromString);
export const flickFormatHost = format(Schema.HostFromString);
export const flickFormatHostPort = format(Schema.HostPortFromString);
export const flickFormatInet = format(Schema.InetAddressFromString);
export const flickFormatUnixPath = format(Schema.UnixPathAddressFromString);
export const flickFormatSocket = format(socket);

/** `Net.format_record`. */
export const flickFormatRecord = (record: Dns.DnsRecord): string => Dns.formatRecord(record);

export const flickDomainName = fallible(Schema.DomainNameFromString);
export const flickHost = fallible(Schema.HostFromString);
export const flickHostPort = fallible(Schema.HostPortFromString);
export const flickInetAddress = fallible(Schema.InetAddressFromString);
export const flickInetAddressV4 = fallible(Schema.InetAddressFromString.check(Schema.makeFilter(Schema.is(Schema.InetAddressV4), { expected: "an IPv4 socket address" })));
export const flickInetAddressV6 = fallible(Schema.InetAddressFromString.check(Schema.makeFilter(Schema.is(Schema.InetAddressV6), { expected: "an IPv6 socket address" })));
export const flickSocketAddress = fallible(socket);
export const flickUnixPathAddress = fallible(Schema.UnixPathAddressFromString);
export const flickIpAddress = fallible(Schema.IpAddressFromString);
export const flickIpInterface = fallible(Schema.IpInterfaceFromString);
export const flickIpLinkLocalAddress = fallible(Schema.IpLinkLocalAddressFromString);
export const flickIpLoopbackAddress = fallible(Schema.IpLoopbackAddressFromString);
export const flickIpMulticastAddress = fallible(Schema.IpMulticastAddressFromString);
export const flickIpNetwork = fallible(Schema.IpNetworkFromString);
export const flickIpUnicastAddress = fallible(Schema.IpUnicastAddressFromString);
export const flickIpUnspecifiedAddress = fallible(Schema.IpUnspecifiedAddressFromString);
export const flickIpv4Address = fallible(Schema.Ipv4AddressFromString);
export const flickIpv4BroadcastAddress = fallible(Schema.Ipv4BroadcastAddressFromString);
export const flickIpv4Interface = fallible(Schema.Ipv4InterfaceFromString);
export const flickIpv4Network = fallible(Schema.Ipv4NetworkFromString);
export const flickIpv4PrivateAddress = fallible(Schema.Ipv4PrivateAddressFromString);
export const flickIpv6Address = fallible(Schema.Ipv6AddressFromString);
export const flickIpv6Interface = fallible(Schema.Ipv6InterfaceFromString);
export const flickIpv6Network = fallible(Schema.Ipv6NetworkFromString);
export const flickIpv6UniqueLocalAddress = fallible(Schema.Ipv6UniqueLocalAddressFromString);
export const flickMacAddress = fallible(Schema.MacAddressFromString);
export const flickMacBroadcastAddress = fallible(Schema.MacBroadcastAddressFromString);
export const flickMacLocallyAdministeredAddress = fallible(Schema.MacLocallyAdministeredAddressFromString);
export const flickMacMulticastAddress = fallible(Schema.MacMulticastAddressFromString);
export const flickMacUnicastAddress = fallible(Schema.MacUnicastAddressFromString);
export const flickMacUniversallyAdministeredAddress = fallible(Schema.MacUniversallyAdministeredAddressFromString);
export const flickScopedIpv6Literal = fallible(Schema.ScopedIpv6LiteralFromString);

/** A DNS record codec: Effect's JSON form, `{ "_tag": "MX", … }`. */
const record = (schema: AnySchema): AnySchema => fallible(Schema.toCodecJson(schema));

export const flickDnsRecord = record(Schema.DnsRecord);
export const flickDnsARecord = record(Schema.DnsARecord);
export const flickDnsAaaaRecord = record(Schema.DnsAaaaRecord);
export const flickDnsCaaRecord = record(Schema.DnsCaaRecord);
export const flickDnsCnameRecord = record(Schema.DnsCnameRecord);
export const flickDnsMxRecord = record(Schema.DnsMxRecord);
export const flickDnsNaptrRecord = record(Schema.DnsNaptrRecord);
export const flickDnsNsRecord = record(Schema.DnsNsRecord);
export const flickDnsPtrRecord = record(Schema.DnsPtrRecord);
export const flickDnsSoaRecord = record(Schema.DnsSoaRecord);
export const flickDnsSrvRecord = record(Schema.DnsSrvRecord);
export const flickDnsTlsaRecord = record(Schema.DnsTlsaRecord);
export const flickDnsTxtRecord = record(Schema.DnsTxtRecord);

/** `Net.dns_record_type`: one of the record type names. */
export const flickDnsRecordType: AnySchema = Schema.toCodecJson(Schema.DnsRecordType);
