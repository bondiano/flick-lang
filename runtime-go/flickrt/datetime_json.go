package flickrt

import "math"

// The JSON codecs of date-times and zones, as Effect Schema's
// DateTimeUtcFromString, DateTimeZonedFromString, TimeZoneFromString and
// their kin read and write them.

// typed is an encoding that first checks the value is what Effect's declared
// schema `name` accepts.
func typed(name string, ok func(Value) bool, f func(Value) Value) func(*Fiber, Value) (Value, []issue) {
	return func(_ *Fiber, v Value) (Value, []issue) {
		if !ok(v) {
			return nil, expected(name, v)
		}
		return f(v), nil
	}
}

func isUtc(v Value) bool       { d, ok := v.(*DateTime); return ok && d.zone == nil }
func isZoned(v Value) bool     { d, ok := v.(*DateTime); return ok && d.zone != nil }
func isNamedZone(v Value) bool { z, ok := v.(*TimeZone); return ok && z.loc != nil }
func isOffsetZone(v Value) bool {
	z, ok := v.(*TimeZone)
	return ok && z.loc == nil
}

var (
	// CodecDateTimeUtc is `DateTime.json_utc`.
	CodecDateTimeUtc Value = &fallible{&stage{
		&stringAs{"a string that will be decoded as a DateTime.Utc"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if d, ok := parseInstant(v.(string)); ok {
				return d, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.Utc", isUtc, DateTimeFormatIso),
	}}

	// CodecDateTimeUtcMillis is `DateTime.json_utc_millis`.
	CodecDateTimeUtcMillis Value = &fallible{&stage{
		withChecks(numberType, isInt()),
		func(_ *Fiber, v Value) (Value, []issue) {
			if x := numberOf(v); math.Abs(x) <= maxTimeValue {
				return &DateTime{ms: timeClip(x)}, nil
			}
			return nil, fail("Invalid DateTime input")
		},
		typed("DateTime.Utc", isUtc, DateTimeToEpochMillis),
	}}

	// CodecDateTimeZoned is `DateTime.json_zoned`.
	CodecDateTimeZoned Value = &fallible{&stage{
		&stringAs{"a zoned DateTime string (e.g. 2024-01-01T00:00:00.000+00:00[Europe/London])"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if d, ok := makeZonedFromString(v.(string)); ok {
				return d, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.Zoned", isZoned, func(v Value) Value { return v.(*DateTime).formatIsoZoned() }),
	}}

	// CodecTimeZone is `DateTime.json_zone`: an IANA name or an offset.
	CodecTimeZone Value = &fallible{&stage{
		&stringAs{"a time zone string (IANA identifier or offset like +03:00)"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if z, ok := zoneFromString(v.(string)); ok {
				return z, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.TimeZone", func(v Value) bool { _, ok := v.(*TimeZone); return ok },
			func(v Value) Value { return v.(*TimeZone).String() }),
	}}

	// CodecTimeZoneNamed is `DateTime.json_zone_named`.
	CodecTimeZoneNamed Value = &fallible{&stage{
		&stringAs{"an IANA time zone identifier"},
		func(_ *Fiber, v Value) (Value, []issue) {
			if z, ok := zoneNamed(v.(string)); ok {
				return z, nil
			}
			return nil, fail("an invalid value")
		},
		typed("DateTime.TimeZone.Named", isNamedZone, func(v Value) Value { return v.(*TimeZone).id }),
	}}

	// CodecTimeZoneOffset is `DateTime.json_zone_offset`: milliseconds.
	CodecTimeZoneOffset Value = &stage{
		CodecNumberFinite.(codec),
		pure(func(v Value) Value { return &TimeZone{offset: numberOf(v)} }),
		typed("DateTime.TimeZone.Offset", isOffsetZone, func(v Value) Value { return v.(*TimeZone).offset }),
	}
)
