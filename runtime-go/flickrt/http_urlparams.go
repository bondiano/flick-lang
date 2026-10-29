package flickrt

import (
	"regexp"
	"strings"
)

// Http.UrlParams as Effect's: pairs of key and value in order, a key
// possibly given more than once. Flick only ever puts strings in them, so
// they are the list of pairs itself, which `==` compares as Effect's
// equivalence does.

var (
	UrlParamsEmpty         Value = []Value{}
	urlParamsNamesKey            = "Http.UrlParams.CurrentRedactedNames"
	defaultUrlRedactedName       = []string{"AWSAccessKeyId", "Signature", "sig", "X-Goog-Signature"}
)

// UrlParamsDefaultRedactedNames is the names Effect masks by default.
var UrlParamsDefaultRedactedNames Value = []Value{"AWSAccessKeyId", "Signature", "sig", "X-Goog-Signature"}

func paramsOf(v Value) []Value { return v.([]Value) }

func pairOf(p Value) (string, string) {
	kv := p.([]Value)
	return kv[0].(string), kv[1].(string)
}

// UrlParamsMake is parameters of the pairs, kept as they are: a Flick list
// is never changed.
func UrlParamsMake(input Value) Value { return List(input) }

func UrlParamsGetAll(key, p Value) Value {
	out := []Value{}
	for _, kv := range paramsOf(p) {
		if k, v := pairOf(kv); k == key {
			out = append(out, v)
		}
	}
	return out
}

func UrlParamsGetFirst(key, p Value) Value {
	for _, kv := range paramsOf(p) {
		if k, v := pairOf(kv); k == key {
			return Some(v)
		}
	}
	return None
}

func UrlParamsGetLast(key, p Value) Value {
	pairs := paramsOf(p)
	for i := len(pairs) - 1; i >= 0; i-- {
		if k, v := pairOf(pairs[i]); k == key {
			return Some(v)
		}
	}
	return None
}

// without is the pairs whose key is not one of `keys`.
func without(pairs []Value, keys map[string]bool) []Value {
	out := make([]Value, 0, len(pairs))
	for _, kv := range pairs {
		if k, _ := pairOf(kv); !keys[k] {
			out = append(out, kv)
		}
	}
	return out
}

// UrlParamsSet replaces every value of the key by one, after the others.
func UrlParamsSet(key, value, p Value) Value {
	out := without(paramsOf(p), map[string]bool{key.(string): true})
	return append(out, []Value{key, value})
}

func UrlParamsTransform(f, p Value) Value { return List(Apply(f, paramsOf(p))) }

// UrlParamsSetAll is the pairs of `input` first, then the pairs of keys
// it does not have.
func UrlParamsSetAll(input, p Value) Value {
	in := List(input)
	keys := map[string]bool{}
	for _, kv := range in {
		k, _ := pairOf(kv)
		keys[k] = true
	}
	return append(append([]Value{}, in...), without(paramsOf(p), keys)...)
}

func UrlParamsAppend(key, value, p Value) Value {
	pairs := paramsOf(p)
	return append(pairs[:len(pairs):len(pairs)], []Value{key, value})
}

func UrlParamsAppendAll(input, p Value) Value {
	pairs := paramsOf(p)
	return append(pairs[:len(pairs):len(pairs)], List(input)...)
}

func UrlParamsRemove(key, p Value) Value {
	return without(paramsOf(p), map[string]bool{key.(string): true})
}

func UrlParamsToString(p Value) Value { return formEncode(paramsOf(p)) }

// UrlParamsToRecord is each key with all its values, keys in the order of
// their first value (a JS Map's order, not an object's).
func UrlParamsToRecord(p Value) Value {
	var keys []string
	values := map[string][]Value{}
	for _, kv := range paramsOf(p) {
		k, v := pairOf(kv)
		if _, seen := values[k]; !seen {
			keys = append(keys, k)
		}
		values[k] = append(values[k], v)
	}
	out := make([]Value, len(keys))
	for i, k := range keys {
		out[i] = []Value{k, values[k]}
	}
	return out
}

// paramsRecord is Effect's `toRecord`: an object of a string per key, or
// an array of strings for a key given more than once.
func paramsRecord(p []Value) *Record {
	out := newRecord(len(p))
	for _, kv := range UrlParamsToRecord(p).([]Value) {
		k, vs := kv.([]Value)[0].(string), kv.([]Value)[1].([]Value)
		if len(vs) == 1 {
			out.put(k, vs[0])
		} else {
			out.put(k, vs)
		}
	}
	return out
}

func UrlParamsToUnknown(p Value) Value { return paramsRecord(paramsOf(p)) }

func UrlParamsToList(p Value) Value { return paramsOf(p) }

func UrlParamsRedact(names, p Value) Value {
	ns := stringsOf(names)
	pairs := paramsOf(p)
	out := make([]Value, len(pairs))
	for i, kv := range pairs {
		k, v := pairOf(kv)
		r := Value(NewData(plainTag, v))
		if redacted(lowerName(k), ns) {
			r = NewData(secretTag, RedactedMake(v))
		}
		out[i] = []Value{k, r}
	}
	return out
}

func UrlParamsIsRedactedName(name, names Value) Value {
	return redacted(lowerName(name), stringsOf(names))
}

func UrlParamsWithRedactedNames(fb *Fiber, names, task Value) Value {
	return withEnv(fb, urlParamsNamesKey, names, task)
}

func UrlParamsEquivalence(a, b Value) Value {
	x, y := paramsOf(a), paramsOf(b)
	if len(x) != len(y) {
		return false
	}
	for i := range x {
		k1, v1 := pairOf(x[i])
		k2, v2 := pairOf(y[i])
		if k1 != k2 || v1 != v2 {
			return false
		}
	}
	return true
}

// UrlParamsShow is `show` of parameters, the default secret names masked.
func UrlParamsShow(p Value) Value {
	var b strings.Builder
	b.WriteString("UrlParams [")
	for i, kv := range paramsOf(p) {
		if i > 0 {
			b.WriteString(", ")
		}
		k, v := pairOf(kv)
		if redacted(lowerName(k), defaultUrlRedactedName) {
			v = "<redacted>"
		}
		b.WriteString("(" + QuoteJSON(k) + ", " + QuoteJSON(v) + ")")
	}
	return b.String() + "]"
}

func UrlParamsRedactUrl(names, url Value) Value { return redactURL(url.(string), stringsOf(names)) }

// Effect's `redactUrl`: it works on the text, so a URL no parser accepts
// is still masked.
var (
	authorityStart      = regexp.MustCompile(`(?i)^(?:(https?|ftp|wss?):[/\\]*|[a-z][a-z\d+.-]*://|[/\\]{2,})`)
	specialAuthorityEnd = regexp.MustCompile(`[/\\?#]`)
	plainAuthorityEnd   = regexp.MustCompile(`[/?#]`)
)

func redactURL(url string, names []string) string {
	if strings.Contains(url, "@") {
		url = redactUserinfo(url)
	}
	start := strings.IndexByte(url, '?')
	if start < 0 || len(names) == 0 {
		return url
	}
	hash := strings.IndexByte(url, '#')
	if hash >= 0 && hash < start {
		return url
	}
	end := len(url)
	if hash >= 0 {
		end = hash
	}
	parts := strings.Split(url[start+1:end], "&")
	for i, part := range parts {
		if eq := strings.IndexByte(part, '='); eq >= 0 && redacted(lowerName(decodeQueryKey(part[:eq])), names) {
			parts[i] = part[:eq+1] + "REDACTED"
		}
	}
	return url[:start+1] + strings.Join(parts, "&") + url[end:]
}

// redactUserinfo masks the user and password; tabs and newlines are left
// out only to find them, as a URL parser would.
func redactUserinfo(url string) string {
	var normalized []byte
	var offsets []int
	for i := 0; i < len(url); i++ {
		c := url[i]
		if c == '\t' || c == '\n' || c == '\r' || len(normalized) == 0 && c <= ' ' {
			continue
		}
		normalized = append(normalized, c)
		offsets = append(offsets, i)
	}
	m := authorityStart.FindSubmatchIndex(normalized)
	if m == nil {
		return url
	}
	start := m[1]
	special := m[2] >= 0 || !strings.Contains(string(normalized[:start]), ":")
	rest := string(normalized[start:])
	end := plainAuthorityEnd
	if special {
		end = specialAuthorityEnd
	}
	if loc := end.FindStringIndex(rest); loc != nil {
		rest = rest[:loc[0]]
	}
	at := strings.LastIndexByte(rest, '@')
	if at < 0 {
		return url
	}
	return url[:offsets[start]] + "REDACTED:REDACTED" + url[offsets[start+at]:]
}

func decodeQueryKey(key string) string {
	if !strings.ContainsAny(key, "%+") {
		return key
	}
	if s, ok := decodeURIComponent(strings.ReplaceAll(key, "+", " ")); ok {
		return s
	}
	return key
}
