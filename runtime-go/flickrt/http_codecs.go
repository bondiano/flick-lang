package flickrt

import (
	"strconv"
	"strings"
)

// Http.Codecs: the JSON codecs Effect Schema gives HTTP values, and those
// that read parameters, cookies, query strings and forms into a record or
// tree for a codec of the program. Their decoding may refuse a wire value
// of the right JSON type, so they are fallible.

// fallible marks a codec whose decoding may refuse a value of the right
// JSON type: it has no exact JSON Schema.
type fallible struct{ codec }

// stage is a codec read from the JSON form `wire`, then by `to`; encoding
// runs `back` first.
type stage struct {
	wire codec
	to   func(*Fiber, Value) (Value, []issue)
	back func(*Fiber, Value) (Value, []issue)
}

func (s *stage) accepts(in Value) bool { return s.wire.accepts(in) }

func (s *stage) decode(fb *Fiber, in Value) (Value, []issue) {
	v, issues := s.wire.decode(fb, in)
	if issues != nil {
		return nil, issues
	}
	return s.to(fb, v)
}

func (s *stage) encode(fb *Fiber, v Value) (Value, []issue) {
	w, issues := s.back(fb, v)
	if issues != nil {
		return nil, issues
	}
	return s.wire.encode(fb, w)
}

func httpCodec(wire codec, to, back func(*Fiber, Value) (Value, []issue)) Value {
	return &fallible{&stage{wire, to, back}}
}

// stringAs is a JSON string that Effect names `expected` when it is not one.
type stringAs struct{ expected string }

func (s *stringAs) accepts(in Value) bool                     { _, ok := in.(string); return ok }
func (s *stringAs) encode(_ *Fiber, v Value) (Value, []issue) { return v, nil }
func (s *stringAs) decode(_ *Fiber, in Value) (Value, []issue) {
	if _, ok := in.(string); !ok {
		return nil, expected(s.expected, in)
	}
	return in, nil
}

// stringRecord is a JSON object of strings.
type stringRecord struct{}

func (stringRecord) accepts(in Value) bool                     { _, ok := in.(*Record); return ok }
func (stringRecord) encode(_ *Fiber, v Value) (Value, []issue) { return v, nil }
func (stringRecord) decode(_ *Fiber, in Value) (Value, []issue) {
	r, ok := in.(*Record)
	if !ok {
		return nil, expected("an object", in)
	}
	for i, v := range r.vals {
		if _, ok := v.(string); !ok {
			return nil, under(r.keys[i], expected("a string", v))
		}
	}
	return r, nil
}

var (
	stringWire  = &stringAs{"a string"}
	stringsWire = sch(SchemaList(SchemaString)).jsonOf()
	pairsWire   = sch(SchemaList(SchemaTuple(SchemaString, SchemaString))).jsonOf()
)

func pure(f func(Value) Value) func(*Fiber, Value) (Value, []issue) {
	return func(_ *Fiber, v Value) (Value, []issue) { return f(v), nil }
}

var (
	CodecNumberFinite Value = sch(SchemaNumber).jsonOf()

	CodecHeaders = httpCodec(stringRecord{}, pure(func(v Value) Value {
		r := record(v)
		out := newRecord(len(r.keys))
		for i, k := range r.keys {
			out.put(lowerName(k), r.vals[i])
		}
		return out
	}), pure(func(h Value) Value { return h }))

	CodecUrlParams = httpCodec(pairsWire, pure(UrlParamsMake), pure(UrlParamsToList))

	CodecCookie = httpCodec(stringWire, func(_ *Fiber, v Value) (Value, []issue) {
		if c, ok := parseSetCookie(Trim(v).(string)); ok {
			return c, nil
		}
		return nil, fail("an invalid value")
	}, pure(CookiesSerializeCookie))

	CodecCookies = httpCodec(stringsWire, pure(CookiesFromSetCookie), pure(CookiesToSetCookieHeaders))

	CodecUrl = httpCodec(&stringAs{"a string that will be decoded as a URL"}, func(_ *Fiber, v Value) (Value, []issue) {
		if u, ok := parseURL(v.(string)); ok {
			return *u, nil
		}
		return nil, fail("an invalid value")
	}, pure(UrlHref))
)

// CodecFromUrlParams reads the parameters as a record of a string per key,
// or an array of them for a key given more than once.
func CodecFromUrlParams(inner Value) Value {
	c := codecOf(inner)
	return httpCodec(pairsWire, func(fb *Fiber, v Value) (Value, []issue) {
		return c.decode(fb, paramsRecord(List(v)))
	}, func(fb *Fiber, v Value) (Value, []issue) {
		w, issues := c.encode(fb, v)
		if issues != nil {
			return nil, issues
		}
		return paramsOfInput(w), nil
	})
}

// CodecFromCookies reads the cookies as a record of their values.
func CodecFromCookies(inner Value) Value {
	c := codecOf(inner)
	return httpCodec(stringsWire, func(fb *Fiber, v Value) (Value, []issue) {
		jar := jarOf(CookiesFromSetCookie(v))
		values := newRecord(len(jar.keys))
		for i, k := range jar.keys {
			values.put(k, cookieOf(jar.vals[i]).value)
		}
		return c.decode(fb, values)
	}, func(fb *Fiber, v Value) (Value, []issue) {
		w, issues := c.encode(fb, v)
		if issues != nil {
			return nil, issues
		}
		r, _ := w.(*Record)
		cookies := []Value{}
		if r != nil {
			for i, k := range r.keys {
				cookie, reason := makeCookie(k, jsString(r.vals[i]), nil)
				if reason != "" {
					panic(Defect{reason})
				}
				cookies = append(cookies, cookie)
			}
		}
		return CookiesToSetCookieHeaders(CookiesFromIterable(cookies)), nil
	})
}

// CodecJsonUrlParamsField reads the JSON text of the first parameter
// `field`.
func CodecJsonUrlParamsField(field, inner Value) Value {
	c, name := codecOf(inner), field.(string)
	return httpCodec(pairsWire, func(fb *Fiber, v Value) (Value, []issue) {
		text := UrlParamsGetFirst(name, List(v))
		if !isSome(text) {
			return nil, under(name, fail("a missing key"))
		}
		parsed := JsonParse(value(text))
		if !isOk(parsed) {
			return nil, fail("an invalid value")
		}
		return c.decode(fb, value(parsed))
	}, func(fb *Fiber, v Value) (Value, []issue) {
		w, issues := c.encode(fb, v)
		if issues != nil {
			return nil, issues
		}
		return []Value{[]Value{name, JsonStringify(w)}}, nil
	})
}

// CodecFromUrlSearchParams reads a query string as a tree of its bracket
// paths.
func CodecFromUrlSearchParams(inner Value) Value {
	c := codecOf(inner)
	return httpCodec(stringWire, func(fb *Fiber, v Value) (Value, []issue) {
		return c.decode(fb, treeRecord(formDecode(strings.TrimPrefix(v.(string), "?"))))
	}, func(fb *Fiber, v Value) (Value, []issue) {
		w, issues := c.encode(fb, v)
		if issues != nil {
			return nil, issues
		}
		return formEncode(bracketEntries(w)), nil
	})
}

// CodecFromFormData reads form fields, as pairs, as a tree of their
// bracket paths.
func CodecFromFormData(inner Value) Value {
	c := codecOf(inner)
	return httpCodec(pairsWire, func(fb *Fiber, v Value) (Value, []issue) {
		fields := List(v)
		pairs := make([]Value, len(fields))
		for i, f := range fields {
			k, val := pairOf(f)
			pairs[i] = []Value{wellFormed(k), wellFormed(val)}
		}
		return c.decode(fb, treeRecord(pairs))
	}, func(fb *Fiber, v Value) (Value, []issue) {
		w, issues := c.encode(fb, v)
		if issues != nil {
			return nil, issues
		}
		return bracketEntries(w), nil
	})
}

// paramsOfInput is Effect's `fromInput` of a record: an array gives a
// pair per item, a nested object a pair per leaf under `key[a][b]`.
func paramsOfInput(w Value) []Value {
	out := []Value{}
	var walk func(path string, v Value)
	walk = func(path string, v Value) {
		switch x := v.(type) {
		case []Value:
			for _, item := range x {
				if item != Unit {
					out = append(out, []Value{path, jsString(item)})
				}
			}
		case *Record:
			for i, k := range x.keys {
				walk(path+"["+k+"]", x.vals[i])
			}
		default:
			if v != Unit {
				out = append(out, []Value{path, jsString(v)})
			}
		}
	}
	if r, ok := w.(*Record); ok {
		for i, k := range r.keys {
			walk(k, r.vals[i])
		}
	}
	return out
}

// jsString is JS's String() of a JSON value.
func jsString(v Value) string {
	switch x := v.(type) {
	case string:
		return x
	case float64:
		if x == 0 {
			return "0"
		}
		return ShowNumber(x)
	case int32:
		return strconv.Itoa(int(x))
	case bool:
		return strconv.FormatBool(x)
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			if e != JsonNull && e != Unit {
				parts[i] = jsString(e)
			}
		}
		return strings.Join(parts, ",")
	case *Record:
		return "[object Object]"
	}
	if v == Unit {
		return "undefined"
	}
	return "null"
}
