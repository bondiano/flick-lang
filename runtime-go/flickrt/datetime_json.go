package flickrt

import "math"

// The JSON codecs of `DateTime`, as Effect's schemas: decoding fails on a
// wire value of the right JSON type that is no date-time or zone, so the
// string ones have no JSON Schema form.

var invalidValue = fail("an invalid value")

func dateTimeCodec(expect string, read func(string) Value, write func(Value) (Value, []issue)) Value {
	return httpCodec(&stringAs{expect}, func(v Value) (Value, []issue) {
		if out := read(v.(string)); out != nil {
			return out, nil
		}
		return nil, invalidValue
	}, write)
}

// valid is the date-time, nil for an invalid one.
func valid(t *DateTime) Value {
	if t == nil || !finite(t.millis) {
		return nil
	}
	return t
}

func utcOnly(v Value) (*DateTime, []issue) {
	if t := dt(v); t.zone == nil {
		return t, nil
	}
	return nil, expected("DateTime.Utc", v)
}

// zoneOnly is a zone of a kind: named or an offset.
func zoneOnly(v Value, named bool) (*TimeZone, []issue) {
	z := tz(v)
	switch {
	case named && z.loc == nil:
		return nil, expected("DateTime.TimeZone.Named", v)
	case !named && z.loc != nil:
		return nil, expected("DateTime.TimeZone.Offset", v)
	}
	return z, nil
}

var (
	DateTimeJsonUtc = dateTimeCodec("a string that will be decoded as a DateTime.Utc",
		func(s string) Value { return valid(fromInput(s)) },
		func(v Value) (Value, []issue) {
			t, bad := utcOnly(v)
			if bad != nil {
				return nil, bad
			}
			return DateTimeFormatIso(t), nil
		})

	DateTimeJsonUtcMillis = httpCodec(numberWire{}, func(v Value) (Value, []issue) {
		n := v.(float64)
		if n != math.Trunc(n) || math.Abs(n) > 1<<53-1 {
			return nil, expected("an integer", v)
		}
		if t := valid(utcAt(timeClip(n))); t != nil {
			return t, nil
		}
		return nil, invalidValue
	}, func(v Value) (Value, []issue) {
		t, bad := utcOnly(v)
		if bad != nil {
			return nil, bad
		}
		return t.millis, nil
	})

	DateTimeJsonZoned = dateTimeCodec("a zoned DateTime string (e.g. 2024-01-01T00:00:00.000+00:00[Europe/London])",
		func(s string) Value {
			if r := DateTimeMakeZonedFromString(s); isOk(r) {
				return value(r)
			}
			return nil
		},
		func(v Value) (Value, []issue) {
			if dt(v).zone == nil {
				return nil, expected("DateTime.Zoned", v)
			}
			return value(DateTimeFormatIsoZoned(v)), nil
		})

	DateTimeJsonZone = dateTimeCodec("a time zone string (IANA identifier or offset like +03:00)",
		func(s string) Value { return zoneValue(zoneFromString(s)) },
		func(v Value) (Value, []issue) { return tz(v).String(), nil })

	DateTimeJsonZoneNamed = dateTimeCodec("an IANA time zone identifier",
		func(s string) Value { return zoneValue(zoneNamed(s)) },
		func(v Value) (Value, []issue) {
			z, bad := zoneOnly(v, true)
			if bad != nil {
				return nil, bad
			}
			return z.id, nil
		})

	// shortcut: encoding a named zone fails as a NaN offset, not as Effect's "expected DateTime.TimeZone.Offset"; give transformed codecs a failing encode if programs show that error
	DateTimeJsonZoneOffset Value = &transformed{
		from: codecOf(CodecNumberFinite),
		to:   func(v Value) (Value, []issue) { return zoneOffset(v.(float64)), nil },
		back: func(v Value) Value {
			if z, bad := zoneOnly(v, false); bad == nil {
				return z.offset
			}
			return math.NaN()
		},
	}
)

func zoneValue(z *TimeZone) Value {
	if z == nil {
		return nil
	}
	return z
}

// numberWire is Effect's Number on the wire: any JSON number.
type numberWire struct{}

func (numberWire) accepts(in Value) bool           { _, ok := number(in); return ok }
func (numberWire) encode(v Value) (Value, []issue) { return v, nil }
func (numberWire) decode(in Value) (Value, []issue) {
	if x, ok := number(in); ok {
		return x, nil
	}
	return nil, expected("a number", in)
}
