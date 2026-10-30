package flickrt

import "strings"

// Http.Url as Effect's over JS's `URL`: each setter gives a changed copy,
// and does what assigning the property of a `URL` does, ignoring a value
// the property refuses.

var urlErrorTag = Tag("Http.Url.UrlError.UrlError")

func urlOf(v Value) webURL { return v.(webURL) }

// UrlFromString is the URL of the text, or Effect's error for it.
func UrlFromString(s Value) Value {
	if u, ok := parseURL(s.(string)); ok {
		return Ok(*u)
	}
	return Err(NewData(urlErrorTag, `Invalid URL: "`+s.(string)+`"`))
}

// UrlMake is the URL of the text with the parameters appended to its query
// and the hash, if given, as its fragment.
func UrlMake(s, params, hash Value) Value {
	u, ok := parseURL(s.(string))
	if !ok {
		return Err(NewData(urlErrorTag, "Invalid URL"))
	}
	if pairs := paramsOf(params); len(pairs) > 0 {
		u.setQueryParams(append(u.searchParams(), pairs...))
	}
	if isSome(hash) {
		u.setHash(value(hash).(string))
	}
	return Ok(*u)
}

func UrlHref(u Value) Value { return urlOf(u).href() }

// UrlShow is the URL with its user, password and the current fiber's
// secret query parameters masked.
func UrlShow(fb *Fiber, u Value) Value {
	return redactURL(urlOf(u).href(), currentUrlRedactedNames(fb))
}

// changed is a copy of the URL changed by `f`, which may refuse the value.
func changed(u Value, f func(*webURL)) Value {
	c := urlOf(u)
	f(&c)
	return c
}

func UrlSetHash(v, u Value) Value {
	return changed(u, func(c *webURL) { c.setHash(v.(string)) })
}

func (u *webURL) setHash(v string) {
	v = withoutTabs(v)
	if v == "" {
		u.fragment, u.hasFragment = "", false
		return
	}
	u.fragment, u.hasFragment = percentEncode(strings.TrimPrefix(v, "#"), fragmentSet), true
}

func UrlSetSearch(v, u Value) Value {
	return changed(u, func(c *webURL) { c.setSearch(v.(string)) })
}

func (u *webURL) setSearch(v string) {
	v = withoutTabs(v)
	if v == "" {
		u.query, u.hasQuery = "", false
		return
	}
	u.query, u.hasQuery = percentEncode(strings.TrimPrefix(v, "?"), u.querySet()), true
}

// cannotHaveCredentials is the URL Standard's "cannot have a username,
// password or port".
func (u webURL) cannotHaveCredentials() bool {
	return !u.hasHost || u.host == "" || u.scheme == "file"
}

func UrlSetUsername(v, u Value) Value {
	return changed(u, func(c *webURL) {
		if !c.cannotHaveCredentials() {
			c.username = percentEncode(withoutTabs(v.(string)), userinfoSet)
		}
	})
}

func UrlSetPassword(v, u Value) Value {
	return changed(u, func(c *webURL) {
		if !c.cannotHaveCredentials() {
			c.password = percentEncode(withoutTabs(RedactedReveal(v).(string)), userinfoSet)
		}
	})
}

func UrlSetPort(v, u Value) Value {
	return changed(u, func(c *webURL) {
		if c.cannotHaveCredentials() {
			return
		}
		// a number reads as JS's String() of it, which writes -0 as 0
		port := "0"
		if x := v.(float64); x != 0 {
			port = ShowNumber(x)
		}
		c.setPort(port, true)
	})
}

// UrlSetHost sets the host and, after a colon, the port; a port it
// refuses leaves the new host set, as JS does.
func UrlSetHost(v, u Value) Value {
	return changed(u, func(c *webURL) { c.setAuthority(v.(string), true) })
}

func UrlSetHostname(v, u Value) Value {
	return changed(u, func(c *webURL) { c.setAuthority(v.(string), false) })
}

func (u *webURL) setAuthority(v string, withPort bool) {
	if u.opaque {
		return
	}
	v = withoutTabs(v)
	if i := strings.IndexAny(v, u.authorityEnds()); i >= 0 {
		v = v[:i]
	}
	host, port, hasPort := cutPort(v)
	if hasPort && !withPort {
		return
	}
	if host == "" && (u.username != "" || u.password != "" || u.port != "") {
		return
	}
	if !u.setHost(host) {
		return
	}
	if hasPort {
		u.setPort(port, true)
	}
}

func UrlSetPathname(v, u Value) Value {
	return changed(u, func(c *webURL) {
		if c.opaque {
			return
		}
		s := withoutTabs(v.(string))
		if !c.hasHost && s == "" {
			c.path = "/"
			return
		}
		c.setPath(s)
	})
}

// UrlSetProtocol changes the scheme to one of the same kind: special
// (`http`, `ws`, …) or not.
func UrlSetProtocol(v, u Value) Value {
	return changed(u, func(c *webURL) {
		s := withoutTabs(v.(string)) + ":"
		end := schemeEnd(s)
		if end < 0 || s[end] != ':' {
			return
		}
		scheme := strings.ToLower(s[:end])
		if isSpecial(scheme) != isSpecial(c.scheme) ||
			scheme == "file" && (c.username != "" || c.password != "" || c.port != "") ||
			c.scheme == "file" && c.host == "" {
			return
		}
		c.scheme = scheme
		if specialPorts[scheme] == c.port {
			c.port = ""
		}
	})
}

// UrlSetHref is the URL of the text; a text that is no URL keeps the URL.
func UrlSetHref(href, u Value) Value {
	if p, ok := parseURL(href.(string)); ok {
		return *p
	}
	return u
}

// searchParams is the pairs of the query, as `url.searchParams` reads it.
func (u webURL) searchParams() []Value {
	if !u.hasQuery {
		return []Value{}
	}
	return formDecode(u.query)
}

// setQueryParams sets the query to the pairs, as `URLSearchParams` writes
// the URL it belongs to.
func (u *webURL) setQueryParams(pairs []Value) {
	q := formEncode(pairs)
	u.query, u.hasQuery = q, q != ""
}

func UrlUrlParams(u Value) Value { return urlOf(u).searchParams() }

func UrlSetUrlParams(params, u Value) Value {
	return changed(u, func(c *webURL) { c.setSearch(formEncode(paramsOf(params))) })
}

func UrlModifyUrlParams(fb *Fiber, f, u Value) Value {
	return changed(u, func(c *webURL) {
		c.setSearch(formEncode(paramsOf(Apply(fb, f, c.searchParams()))))
	})
}
