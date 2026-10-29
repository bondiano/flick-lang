package flickrt

import (
	"math"
	"strconv"
	"strings"
	"time"
)

// Reading cookies from headers, as Effect's `fromSetCookie` and
// `parseHeader` do.

func CookiesFromSetCookie(headers Value) Value {
	jar := &Record{}
	for _, h := range List(headers) {
		if c, ok := parseSetCookie(Trim(h).(string)); ok {
			jar.put(c.name, c)
		}
	}
	return jar
}

func tryDecodeURIComponent(s string) string {
	if d, ok := decodeURIComponent(s); ok {
		return d
	}
	return s
}

// parseSetCookie is the cookie of a `Set-Cookie` value, if any; an
// attribute it cannot read is left out.
func parseSetCookie(header string) (Cookie, bool) {
	var parts []string
	for _, p := range strings.Split(header, ";") {
		if p = Trim(p).(string); p != "" {
			parts = append(parts, p)
		}
	}
	if len(parts) == 0 {
		return Cookie{}, false
	}
	name, encoded, ok := strings.Cut(parts[0], "=")
	if !ok || !validName(name) {
		return Cookie{}, false
	}
	c := Cookie{name: name, value: tryDecodeURIComponent(encoded), encoded: encoded}
	if len(parts) == 1 {
		return c, true
	}
	for _, part := range parts[1:] {
		key, val, hasValue := strings.Cut(part, "=")
		readAttribute(&c.opts, ToLowerCase(Trim(key)).(string), Trim(val).(string), hasValue)
	}
	c.options = optionsUndefined
	if c.opts.keys != 0 {
		c.options = optionsSet
	}
	return c, true
}

// readAttribute reads one attribute into `o`, which gets the key only
// when it reads a value.
func readAttribute(o *cookieOptions, key, val string, hasValue bool) {
	switch key {
	case "httponly":
		o.httpOnly = true
		o.with(optHttpOnly)
		return
	case "secure":
		o.secure = true
		o.with(optSecure)
		return
	case "partitioned":
		o.partitioned = true
		o.with(optPartitioned)
		return
	}
	if !hasValue {
		return
	}
	switch key {
	case "domain":
		if d := strings.TrimPrefix(val, "."); d != "" {
			o.domain = d
			o.with(optDomain)
		}
	case "expires":
		if t, ok := parseDate(val); ok {
			o.expires = t.UnixMilli()
			o.with(optExpires)
		}
	case "max-age":
		if n, ok := parseLeadingInt(val); ok {
			o.maxAge = n
			o.with(optMaxAge)
		}
	case "path":
		if strings.HasPrefix(val, "/") {
			o.path = val
			o.with(optPath)
		}
	case "priority":
		if p := map[string]string{"low": "Low", "medium": "Medium", "high": "High"}[ToLowerCase(val).(string)]; p != "" {
			o.priority = p
			o.with(optPriority)
		}
	case "samesite":
		if s := map[string]string{"lax": "Lax", "strict": "Strict", "none": "None"}[ToLowerCase(val).(string)]; s != "" {
			o.sameSite = s
			o.with(optSameSite)
		}
	}
}

// parseLeadingInt is JS's parseInt(s, 10): the integer its leading
// digits, after an optional sign, give.
func parseLeadingInt(s string) (float64, bool) {
	s = TrimStart(s).(string)
	sign := 1.0
	if s != "" && (s[0] == '-' || s[0] == '+') {
		if s[0] == '-' {
			sign = -1
		}
		s = s[1:]
	}
	n := 0
	for n < len(s) && isDigit(s[n]) {
		n++
	}
	if n == 0 {
		return 0, false
	}
	x, _ := strconv.ParseFloat(s[:n], 64)
	return math.Copysign(x, sign), true
}

// parseDate reads the date of an `Expires` attribute.
//
// shortcut: reads the HTTP date forms, asctime and ISO 8601, where JS's Date reads more; two-digit years 50 to 68 fall in the 2000s here, the 1900s there
func parseDate(s string) (time.Time, bool) {
	for _, layout := range []string{
		time.RFC1123, "Mon, 02-Jan-06 15:04:05 MST", "Mon, 02-Jan-2006 15:04:05 MST", time.RFC850,
		time.ANSIC, time.RFC3339Nano, "2006-01-02T15:04", "2006-01-02", "2006-01", "2006",
	} {
		if t, err := time.Parse(layout, s); err == nil {
			return t, true
		}
	}
	return time.Time{}, false
}

// CookiesParseHeader is the names and values of a `Cookie` header, the
// first value of a name kept.
func CookiesParseHeader(header Value) Value {
	h := header.(string)
	pairs := &Record{}
	for pos := 0; pos < len(h); {
		end := strings.IndexByte(h[pos:], ';')
		if end < 0 {
			end = len(h)
		} else {
			end += pos
		}
		eq := strings.IndexByte(h[pos:], '=')
		if eq < 0 {
			break
		}
		eq += pos
		if eq > end {
			pos = end + 1
			continue
		}
		key := Trim(h[pos:eq]).(string)
		if pairs.index(key) < 0 {
			pairs.put(key, cookieHeaderValue(h, eq+1, end))
		}
		pos = end + 1
	}
	out := make([]Value, len(pairs.keys))
	for i, k := range pairs.keys {
		out[i] = []Value{k, RedactedMake(pairs.vals[i])}
	}
	return out
}

// cookieHeaderValue is the value between `start` and `end`: a quoted one
// loses its first and last UTF-16 unit, as Effect slices it.
func cookieHeaderValue(h string, start, end int) string {
	val := h[start:end]
	if start < len(h) && h[start] == '"' {
		if start+1 == end {
			val = `"`
		} else {
			units := utf16Units(h[start+1 : end])
			val = fromUnits(units[:len(units)-1])
		}
	}
	val = Trim(val).(string)
	if strings.Contains(val, "%") {
		return tryDecodeURIComponent(val)
	}
	return val
}
