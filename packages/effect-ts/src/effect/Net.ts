// Effect's network schemas for the bindings of the standard library (`Net`):
// an address codec decodes text to Effect's address value, a DNS record
// codec to Effect's record; both are opaque to Flick.
import { type Result, Schema } from "effect";
import { Dns } from "effect/net";

import type { AnySchema } from "../schema/schemas.ts";
import { type SchemaError, codecDecodeResult } from "../schema/index.ts";
import { fallible } from "./Schema.ts";

/** The canonical text of an address, by the codec it was decoded with. */
const format = <A>(codec: Schema.Codec<A, string>): ((a: A) => string) => Schema.encodeSync(codec);

const socket = /*#__PURE__*/ Schema.Union([Schema.InetAddressFromString, Schema.UnixPathAddressFromString]);

export const flickFormatIp = /*#__PURE__*/ format(Schema.IpAddressFromString);
export const flickFormatInterface = /*#__PURE__*/ format(Schema.IpInterfaceFromString);
export const flickFormatNetwork = /*#__PURE__*/ format(Schema.IpNetworkFromString);
export const flickFormatMac = /*#__PURE__*/ format(Schema.MacAddressFromString);
export const flickFormatDomain = /*#__PURE__*/ format(Schema.DomainNameFromString);
export const flickFormatScoped = /*#__PURE__*/ format(Schema.ScopedIpv6LiteralFromString);
export const flickFormatHost = /*#__PURE__*/ format(Schema.HostFromString);
export const flickFormatHostPort = /*#__PURE__*/ format(Schema.HostPortFromString);
export const flickFormatInet = /*#__PURE__*/ format(Schema.InetAddressFromString);
export const flickFormatUnixPath = /*#__PURE__*/ format(Schema.UnixPathAddressFromString);
export const flickFormatSocket = /*#__PURE__*/ format(socket);

/** `Net.format_record`. */
export const flickFormatRecord = (record: Dns.DnsRecord): string => Dns.formatRecord(record);

export const flickDomainName = /*#__PURE__*/ fallible(Schema.DomainNameFromString);
export const flickHost = /*#__PURE__*/ fallible(Schema.HostFromString);
export const flickHostPort = /*#__PURE__*/ fallible(Schema.HostPortFromString);
export const flickInetAddress = /*#__PURE__*/ fallible(Schema.InetAddressFromString);
export const flickInetAddressV4 = /*#__PURE__*/ fallible(Schema.InetAddressFromString.check(Schema.makeFilter(Schema.is(Schema.InetAddressV4), { expected: "an IPv4 socket address" })));
export const flickInetAddressV6 = /*#__PURE__*/ fallible(Schema.InetAddressFromString.check(Schema.makeFilter(Schema.is(Schema.InetAddressV6), { expected: "an IPv6 socket address" })));
export const flickSocketAddress = /*#__PURE__*/ fallible(socket);
export const flickUnixPathAddress = /*#__PURE__*/ fallible(Schema.UnixPathAddressFromString);
export const flickIpAddress = /*#__PURE__*/ fallible(Schema.IpAddressFromString);
export const flickIpInterface = /*#__PURE__*/ fallible(Schema.IpInterfaceFromString);
export const flickIpLinkLocalAddress = /*#__PURE__*/ fallible(Schema.IpLinkLocalAddressFromString);
export const flickIpLoopbackAddress = /*#__PURE__*/ fallible(Schema.IpLoopbackAddressFromString);
export const flickIpMulticastAddress = /*#__PURE__*/ fallible(Schema.IpMulticastAddressFromString);
export const flickIpNetwork = /*#__PURE__*/ fallible(Schema.IpNetworkFromString);
export const flickIpUnicastAddress = /*#__PURE__*/ fallible(Schema.IpUnicastAddressFromString);
export const flickIpUnspecifiedAddress = /*#__PURE__*/ fallible(Schema.IpUnspecifiedAddressFromString);
export const flickIpv4Address = /*#__PURE__*/ fallible(Schema.Ipv4AddressFromString);
export const flickIpv4BroadcastAddress = /*#__PURE__*/ fallible(Schema.Ipv4BroadcastAddressFromString);
export const flickIpv4Interface = /*#__PURE__*/ fallible(Schema.Ipv4InterfaceFromString);
export const flickIpv4Network = /*#__PURE__*/ fallible(Schema.Ipv4NetworkFromString);
export const flickIpv4PrivateAddress = /*#__PURE__*/ fallible(Schema.Ipv4PrivateAddressFromString);
export const flickIpv6Address = /*#__PURE__*/ fallible(Schema.Ipv6AddressFromString);
export const flickIpv6Interface = /*#__PURE__*/ fallible(Schema.Ipv6InterfaceFromString);
export const flickIpv6Network = /*#__PURE__*/ fallible(Schema.Ipv6NetworkFromString);
export const flickIpv6UniqueLocalAddress = /*#__PURE__*/ fallible(Schema.Ipv6UniqueLocalAddressFromString);
export const flickMacAddress = /*#__PURE__*/ fallible(Schema.MacAddressFromString);
export const flickMacBroadcastAddress = /*#__PURE__*/ fallible(Schema.MacBroadcastAddressFromString);
export const flickMacLocallyAdministeredAddress = /*#__PURE__*/ fallible(Schema.MacLocallyAdministeredAddressFromString);
export const flickMacMulticastAddress = /*#__PURE__*/ fallible(Schema.MacMulticastAddressFromString);
export const flickMacUnicastAddress = /*#__PURE__*/ fallible(Schema.MacUnicastAddressFromString);
export const flickMacUniversallyAdministeredAddress = /*#__PURE__*/ fallible(Schema.MacUniversallyAdministeredAddressFromString);
export const flickScopedIpv6Literal = /*#__PURE__*/ fallible(Schema.ScopedIpv6LiteralFromString);

/** A DNS record codec: Effect's JSON form, `{ "_tag": "MX", … }`. */
const record = (schema: AnySchema): AnySchema => fallible(Schema.toCodecJson(schema));

export const flickDnsRecord = /*#__PURE__*/ record(Schema.DnsRecord);
export const flickDnsARecord = /*#__PURE__*/ record(Schema.DnsARecord);
export const flickDnsAaaaRecord = /*#__PURE__*/ record(Schema.DnsAaaaRecord);
export const flickDnsCaaRecord = /*#__PURE__*/ record(Schema.DnsCaaRecord);
export const flickDnsCnameRecord = /*#__PURE__*/ record(Schema.DnsCnameRecord);
export const flickDnsMxRecord = /*#__PURE__*/ record(Schema.DnsMxRecord);
export const flickDnsNaptrRecord = /*#__PURE__*/ record(Schema.DnsNaptrRecord);
export const flickDnsNsRecord = /*#__PURE__*/ record(Schema.DnsNsRecord);
export const flickDnsPtrRecord = /*#__PURE__*/ record(Schema.DnsPtrRecord);
export const flickDnsSoaRecord = /*#__PURE__*/ record(Schema.DnsSoaRecord);
export const flickDnsSrvRecord = /*#__PURE__*/ record(Schema.DnsSrvRecord);
export const flickDnsTlsaRecord = /*#__PURE__*/ record(Schema.DnsTlsaRecord);
export const flickDnsTxtRecord = /*#__PURE__*/ record(Schema.DnsTxtRecord);

/** `Net.dns_record_type`: one of the record type names. */
export const flickDnsRecordType: AnySchema = /*#__PURE__*/ Schema.toCodecJson(Schema.DnsRecordType);

/** `Net.parse_*`: the text decoded by the address codec. */
const parse = (codec: AnySchema) => (text: string): Result.Result<unknown, SchemaError> => codecDecodeResult(codec, text);

export const flickParseIp = /*#__PURE__*/ parse(flickIpAddress);
export const flickParseInterface = /*#__PURE__*/ parse(flickIpInterface);
export const flickParseNetwork = /*#__PURE__*/ parse(flickIpNetwork);
export const flickParseMac = /*#__PURE__*/ parse(flickMacAddress);
export const flickParseDomain = /*#__PURE__*/ parse(flickDomainName);
export const flickParseScoped = /*#__PURE__*/ parse(flickScopedIpv6Literal);
export const flickParseHost = /*#__PURE__*/ parse(flickHost);
export const flickParseHostPort = /*#__PURE__*/ parse(flickHostPort);
export const flickParseInet = /*#__PURE__*/ parse(flickInetAddress);
export const flickParseUnixPath = /*#__PURE__*/ parse(flickUnixPathAddress);
export const flickParseSocket = /*#__PURE__*/ parse(flickSocketAddress);
