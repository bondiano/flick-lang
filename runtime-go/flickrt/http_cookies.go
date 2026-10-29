package flickrt

import (
	"math"
	"strings"
	"time"
)

// Http.Cookies as Effect's: cookies by name in the order JS keeps an
// object's keys, each with its value as given and as it goes on the wire.
// A cookie is a Go value of comparable fields and the jar a record of
// them, so `==` compares them as Effect's structural equality does: that
// tells an option given as undefined from one left out.

type Cookie struct {
	name, value, encoded string
	options              uint8 // optionsNone, optionsUndefined or optionsSet
	opts                 cookieOptions
}

const (
	optionsNone      = iota // no `options` key, as a header without attributes gives
	optionsUndefined        // an `options` key that is undefined
	optionsSet
)

// The options of Effect's cookie object, as bits of `keys` (which keys it
// has) and `set` (which have a value).
const (
	optDomain = 1 << iota
	optExpires
	optMaxAge
	optPath
	optPriority
	optHttpOnly
	optSecure
	optPartitioned
	optSameSite
	optAll = 1<<iota - 1
)

type cookieOptions struct {
	keys, set                     uint16
	domain, path                  string
	maxAge                        float64 // seconds, infinite for an infinite duration
	expires                       int64   // milliseconds since the epoch
	priority, sameSite            string  // the attribute's text
	httpOnly, secure, partitioned bool
}

func (o *cookieOptions) has(bit uint16) bool { return o.set&bit != 0 }

// with is the options with the option `bit` given.
func (o *cookieOptions) with(bit uint16) { o.keys, o.set = o.keys|bit, o.set|bit }

var (
	CookiesEmpty    Value = &Record{}
	cookiesErrorTag       = Tag("Http.Cookies.CookiesError.CookiesError")
)

func cookieOf(v Value) Cookie { return v.(Cookie) }
func jarOf(v Value) *Record   { return v.(*Record) }

func (c *Cookie) attributes() *cookieOptions {
	if c.options != optionsSet {
		return nil
	}
	return &c.opts
}

func failCookie(reason string) Value {
	return Fail(NewData(cookiesErrorTag, Nullary(Tag("Http.Cookies.CookiesErrorReason."+reason))))
}

// every is whether `s` is not empty and each of its UTF-16 units is in
// `ok`, as a JS regexp `^[…]+$` tests.
func every(s string, ok func(r rune) bool) bool {
	for len(s) > 0 {
		r, n := decode(s)
		if !ok(r) {
			return false
		}
		s = s[n:]
	}
	return true
}

func cookieNameRune(r rune) bool {
	return r < 0x80 && (isAlpha(byte(r)) || isDigit(byte(r)) || strings.ContainsRune("!#$%&'*+-.^_`|~", r))
}

func validName(s string) bool { return s != "" && every(s, cookieNameRune) }

// validate is Effect's check of a cookie: the reason it is refused, if.
func validate(name, encoded string, o *cookieOptions) string {
	if !validName(name) {
		return "InvalidCookieName"
	}
	if !every(encoded, func(r rune) bool { return r == '\t' || r >= 0x20 && r <= 0x7E || r >= 0x80 && r <= 0xFF }) {
		return "InvalidCookieValue"
	}
	if o == nil {
		return ""
	}
	if o.has(optDomain) && (o.domain == "" ||
		!every(o.domain, func(r rune) bool { return r == '\t' || r >= 0x20 && r <= 0x7E && r != ';' || r >= 0x80 && r <= 0xFF })) {
		return "InvalidCookieDomain"
	}
	if o.has(optPath) && (o.path == "" || !every(o.path, func(r rune) bool { return r >= 0x20 && r <= 0x7E && r != ';' })) {
		return "InvalidCookiePath"
	}
	if o.has(optMaxAge) && math.IsInf(o.maxAge, 0) {
		return "CookieInfinityMaxAge"
	}
	return ""
}

// optionsOf is Effect's options of the Flick attributes: an object with
// every key, those not given undefined.
func optionsOf(attributes Value) cookieOptions {
	a := record(attributes)
	if isSome(a.Get("expires")) {
		panic(Defect{"a cookie's `expires` is a DateTime, which the go target does not have"})
	}
	o := cookieOptions{keys: optAll}
	if d := a.Get("domain"); isSome(d) {
		o.domain = value(d).(string)
		o.with(optDomain)
	}
	if p := a.Get("path"); isSome(p) {
		o.path = value(p).(string)
		o.with(optPath)
	}
	if m := a.Get("maxAge"); isSome(m) {
		o.maxAge = math.Inf(1)
		if s := DurationToSeconds(value(m)); isSome(s) {
			o.maxAge = value(s).(float64)
		}
		o.with(optMaxAge)
	}
	if p := a.Get("priority"); isSome(p) {
		o.priority = ConstructorName(value(p).(*Data).Tag)
		o.with(optPriority)
	}
	if s := a.Get("sameSite"); isSome(s) {
		o.sameSite = map[string]string{"Lax": "Lax", "Strict": "Strict", "NoSameSite": "None"}[ConstructorName(value(s).(*Data).Tag)]
		o.with(optSameSite)
	}
	o.httpOnly = a.Get("httpOnly").(bool)
	o.secure = a.Get("secure").(bool)
	o.partitioned = a.Get("partitioned").(bool)
	return o
}

// makeCookie is Effect's `makeCookie`: the cookie, or the reason it is
// refused.
func makeCookie(name, val string, o *cookieOptions) (Cookie, string) {
	encoded, ok := encodeURIComponent(val)
	if !ok {
		panic(Defect{"URIError: URI malformed"})
	}
	if reason := validate(name, encoded, o); reason != "" {
		return Cookie{}, reason
	}
	if o == nil {
		return Cookie{name, val, encoded, optionsUndefined, cookieOptions{}}, ""
	}
	return Cookie{name, val, encoded, optionsSet, *o}, ""
}

func mustCookie(name, val string, o *cookieOptions) Cookie {
	c, reason := makeCookie(name, val, o)
	if reason != "" {
		failCookie(reason)
	}
	return c
}

func CookiesMakeCookie(name, val, attributes Value) Value {
	o := optionsOf(attributes)
	return mustCookie(name.(string), RedactedReveal(val).(string), &o)
}

func CookiesIsEmpty(c Value) Value { return len(jarOf(c).keys) == 0 }

func CookiesFromIterable(list Value) Value {
	jar := newRecord(len(List(list)))
	for _, c := range List(list) {
		jar.put(cookieOf(c).name, c)
	}
	return jar
}

func CookiesFromReadonlyRecord(pairs Value) Value {
	jar := newRecord(len(List(pairs)))
	for _, p := range List(pairs) {
		kv := p.([]Value)
		jar.put(kv[0].(string), kv[1])
	}
	return jar
}

func CookiesSetCookie(c, jar Value) Value {
	return jarOf(jar).set(cookieOf(c).name, c)
}

func CookiesSetAllCookie(list, jar Value) Value {
	out := jarOf(jar)
	for _, c := range List(list) {
		out = out.set(cookieOf(c).name, c)
	}
	return out
}

// CookiesMerge is `self` with the cookies of `that` set over it.
func CookiesMerge(that, self Value) Value {
	out, w := jarOf(self), jarOf(that)
	for i, k := range w.keys {
		out = out.set(k, w.vals[i])
	}
	return out
}

func CookiesRemove(name, jar Value) Value { return jarOf(jar).without(name.(string)) }

func CookiesGet(name, jar Value) Value {
	r := jarOf(jar)
	if i := r.index(name.(string)); i >= 0 {
		return Some(r.vals[i])
	}
	return None
}

func CookiesGetValue(name, jar Value) Value {
	c := CookiesGet(name, jar)
	if !isSome(c) {
		return None
	}
	return Some(RedactedMake(cookieOf(value(c)).value))
}

func CookiesSet(name, val, attributes, jar Value) Value {
	o := optionsOf(attributes)
	c := mustCookie(name.(string), RedactedReveal(val).(string), &o)
	return jarOf(jar).set(c.name, c)
}

// CookiesSetAll sets each cookie, failing on the first refused one.
func CookiesSetAll(entries, jar Value) Value {
	out := jarOf(jar)
	for _, e := range List(entries) {
		t := e.([]Value)
		o := optionsOf(t[2])
		c := mustCookie(t[0].(string), RedactedReveal(t[1]).(string), &o)
		out = out.set(c.name, c)
	}
	return out
}

// CookiesExpireCookie sets an empty cookie that expired at the epoch, with
// a `Max-Age` of 0.
func CookiesExpireCookie(name, attributes, jar Value) Value {
	o := optionsOf(record(attributes).set("expires", None).set("maxAge", None))
	o.maxAge, o.expires = 0, 0
	o.with(optMaxAge | optExpires)
	c := mustCookie(name.(string), "", &o)
	return jarOf(jar).set(c.name, c)
}

// serialize is the `Set-Cookie` header value of the cookie.
func (c Cookie) serialize() string {
	o := c.attributes()
	if reason := validate(c.name, c.encoded, o); reason != "" {
		panic(Defect{reason})
	}
	s := c.name + "=" + c.encoded
	if o == nil {
		return s
	}
	if o.has(optMaxAge) {
		if n := math.Trunc(o.maxAge); n != 0 {
			s += "; Max-Age=" + ShowNumber(n)
		} else {
			s += "; Max-Age=0"
		}
	}
	if o.has(optDomain) {
		s += "; Domain=" + o.domain
	}
	if o.has(optPath) {
		s += "; Path=" + o.path
	}
	if o.has(optPriority) {
		s += "; Priority=" + o.priority
	}
	if o.has(optExpires) {
		s += "; Expires=" + time.UnixMilli(o.expires).UTC().Format("Mon, 02 Jan 2006 15:04:05 GMT")
	}
	if o.httpOnly {
		s += "; HttpOnly"
	}
	if o.secure {
		s += "; Secure"
	}
	if o.partitioned {
		s += "; Partitioned"
	}
	if o.has(optSameSite) {
		s += "; SameSite=" + o.sameSite
	}
	return s
}

func CookiesSerializeCookie(c Value) Value { return cookieOf(c).serialize() }

func CookiesToCookieHeader(jar Value) Value {
	vals := jarOf(jar).vals
	parts := make([]string, len(vals))
	for i, c := range vals {
		parts[i] = cookieOf(c).name + "=" + cookieOf(c).encoded
	}
	return strings.Join(parts, "; ")
}

func CookiesToRecord(jar Value) Value {
	vals := jarOf(jar).vals
	out := make([]Value, len(vals))
	for i, c := range vals {
		out[i] = []Value{cookieOf(c).name, RedactedMake(cookieOf(c).value)}
	}
	return out
}

func CookiesToSetCookieHeaders(jar Value) Value {
	vals := jarOf(jar).vals
	out := make([]Value, len(vals))
	for i, c := range vals {
		out[i] = cookieOf(c).serialize()
	}
	return out
}

func CookiesName(c Value) Value  { return cookieOf(c).name }
func CookiesValue(c Value) Value { return RedactedMake(cookieOf(c).value) }

// CookiesShowCookie is the `Set-Cookie` form with the value masked.
func CookiesShowCookie(c Value) Value {
	s := cookieOf(c).serialize()
	attributes := ""
	if i := strings.IndexByte(s, ';'); i >= 0 {
		attributes = s[i:]
	}
	return "Cookie " + QuoteJSON(cookieOf(c).name+"=<redacted>"+attributes)
}

func CookiesShowCookies(jar Value) Value {
	vals := jarOf(jar).vals
	parts := make([]string, len(vals))
	for i, c := range vals {
		parts[i] = CookiesShowCookie(c).(string)
	}
	return "Cookies [" + strings.Join(parts, ", ") + "]"
}
