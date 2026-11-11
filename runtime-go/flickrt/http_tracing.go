package flickrt

import (
	"strconv"
	"strings"
)

// The attributes the HTTP server and client spans record of a URL and of
// headers.

var tracingDefaultPorts = map[string]int32{"http": 80, "https": 443, "ws": 80, "wss": 443}

// addURLAttributes records the URL's `url.*` and `server.*` attributes,
// the query parameters of `names` masked.
func (s *span) addURLAttributes(u webURL, names []string) {
	search := ""
	if u.hasQuery && u.query != "" {
		search = "?" + u.query
	}
	query := strings.TrimPrefix(redactURL(search, names), "?")
	s.attribute("url.full", redactURL(u.href(), names))
	s.attribute("url.path", u.path)
	if query != "" {
		s.attribute("url.query", query)
	}
	s.attribute("url.scheme", u.scheme)
	if u.host == "" {
		return
	}
	// URL keeps IPv6 brackets in the hostname, but the semantic conventions do not.
	s.attribute("server.address", strings.TrimSuffix(strings.TrimPrefix(u.host, "["), "]"))
	if u.port == "" {
		if port, ok := tracingDefaultPorts[u.scheme]; ok {
			s.attribute("server.port", port)
		}
	} else if port, err := strconv.Atoi(u.port); err == nil {
		s.attribute("server.port", int32(port))
	}
}

// addHeaderAttributes records the headers the filter of `filterKey`
// passes, as `http.<phase>.header.<name>`, the redacted ones masked.
func (s *span) addHeaderAttributes(fb *Fiber, phase string, headers *Record, filterKey string) {
	filter, ok := fb.env.get(filterKey)
	if !ok {
		return // the default filter passes none
	}
	names := defaultRedactedName
	if ns, ok := fb.env.get(redactedNamesKey); ok {
		names = stringsOf(ns)
	}
	for i, name := range headers.keys {
		if !Apply2(fb, filter, name, phase).(bool) {
			continue
		}
		v := headers.vals[i]
		if redacted(name, names) {
			v = "<redacted>"
		}
		s.attribute("http."+phase+".header."+name, v)
	}
}
