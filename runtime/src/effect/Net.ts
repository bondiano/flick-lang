// Effect's network schemas for the bindings of the standard library (`Net`):
// an address codec keeps the canonical text of the address, a DNS record
// codec Effect's record, opaque to Flick.
import { Result, Schema, SchemaTransformation } from "effect";
import { Dns } from "effect/net";

import type { AnySchema } from "../schema/schemas.ts";
import { fallible } from "./Schema.ts";

/** The canonical text of what `codec` decodes, both ways; invalid text is a `SchemaError`. */
const text = <A>(codec: Schema.Codec<A, string>): AnySchema =>
  fallible(
    codec.pipe(
      Schema.decodeTo(
        Schema.String.check(Schema.makeFilter((s: string) => Result.isSuccess(Schema.decodeResult(codec)(s)))),
        SchemaTransformation.transform({
          decode: (a: A): string => Schema.encodeSync(codec)(a),
          encode: (s: string): A => Schema.decodeSync(codec)(s),
        }),
      ),
    ),
  );

/** `Net.format_record`. */
export const flickFormatRecord = (record: Dns.DnsRecord): string => Dns.formatRecord(record);

export const flickDomainName = text(Schema.DomainNameFromString);
export const flickHost = text(Schema.HostFromString);
export const flickHostPort = text(Schema.HostPortFromString);
export const flickInetAddress = text(Schema.InetAddressFromString);
export const flickInetAddressV4 = text(Schema.InetAddressFromString.check(Schema.makeFilter(Schema.is(Schema.InetAddressV4), { expected: "an IPv4 socket address" })));
export const flickInetAddressV6 = text(Schema.InetAddressFromString.check(Schema.makeFilter(Schema.is(Schema.InetAddressV6), { expected: "an IPv6 socket address" })));
export const flickSocketAddress = text(Schema.Union([Schema.InetAddressFromString, Schema.UnixPathAddressFromString]));
export const flickUnixPathAddress = text(Schema.UnixPathAddressFromString);
export const flickIpAddress = text(Schema.IpAddressFromString);
export const flickIpInterface = text(Schema.IpInterfaceFromString);
export const flickIpLinkLocalAddress = text(Schema.IpLinkLocalAddressFromString);
export const flickIpLoopbackAddress = text(Schema.IpLoopbackAddressFromString);
export const flickIpMulticastAddress = text(Schema.IpMulticastAddressFromString);
export const flickIpNetwork = text(Schema.IpNetworkFromString);
export const flickIpUnicastAddress = text(Schema.IpUnicastAddressFromString);
export const flickIpUnspecifiedAddress = text(Schema.IpUnspecifiedAddressFromString);
export const flickIpv4Address = text(Schema.Ipv4AddressFromString);
export const flickIpv4BroadcastAddress = text(Schema.Ipv4BroadcastAddressFromString);
export const flickIpv4Interface = text(Schema.Ipv4InterfaceFromString);
export const flickIpv4Network = text(Schema.Ipv4NetworkFromString);
export const flickIpv4PrivateAddress = text(Schema.Ipv4PrivateAddressFromString);
export const flickIpv6Address = text(Schema.Ipv6AddressFromString);
export const flickIpv6Interface = text(Schema.Ipv6InterfaceFromString);
export const flickIpv6Network = text(Schema.Ipv6NetworkFromString);
export const flickIpv6UniqueLocalAddress = text(Schema.Ipv6UniqueLocalAddressFromString);
export const flickMacAddress = text(Schema.MacAddressFromString);
export const flickMacBroadcastAddress = text(Schema.MacBroadcastAddressFromString);
export const flickMacLocallyAdministeredAddress = text(Schema.MacLocallyAdministeredAddressFromString);
export const flickMacMulticastAddress = text(Schema.MacMulticastAddressFromString);
export const flickMacUnicastAddress = text(Schema.MacUnicastAddressFromString);
export const flickMacUniversallyAdministeredAddress = text(Schema.MacUniversallyAdministeredAddressFromString);
export const flickScopedIpv6Literal = text(Schema.ScopedIpv6LiteralFromString);

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
