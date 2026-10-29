package flickrt

import "strings"

// Http.Router as Effect's HttpRouter over find-my-way: a request goes to
// the route whose pattern matches its path, a static segment before a
// parameter before a wildcard, segment by segment, the first route of
// equal ones; a route under a prefix sees the URL without it.
//
// shortcut: parameters are whole segments (`:id`); find-my-way's parameters inside a segment (`:a-:b`) and regular expressions match nothing, port its tree when a program uses them

var (
	routeContextKey = "Http.Router.RouteContext"
	routeMissingTag = Tag("Http.ServerError.ServerErrorReason.RouteNotFound")
)

const (
	segStatic = iota
	segParam
	segWildcard
)

type segment struct {
	kind int
	text string // the static text, or the parameter's name
}

// A route: its method ("" for every one), its path, the prefix it was put
// under, and the segments of its pattern without the wildcard.
type route struct {
	method    string
	path      string
	prefix    string
	hasPrefix bool
	segs      []segment
	wildcard  bool
	handler   Value
}

func newRoute(method, path, prefix string, hasPrefix bool, handler Value) *route {
	rt := &route{method: method, path: path, prefix: prefix, hasPrefix: hasPrefix, handler: handler}
	pattern := collapseSlashes(path)
	if pattern == "*" {
		pattern = "/*"
	}
	if strings.HasSuffix(pattern, "/*") {
		rt.wildcard, pattern = true, pattern[:len(pattern)-2]
	}
	for _, p := range strings.Split(strings.TrimPrefix(pattern, "/"), "/") {
		switch {
		case strings.HasPrefix(p, ":") && !strings.HasPrefix(p, "::"):
			rt.segs = append(rt.segs, segment{segParam, p[1:]})
		default:
			rt.segs = append(rt.segs, segment{segStatic, strings.ReplaceAll(p, "::", ":")})
		}
	}
	if rt.wildcard && len(rt.segs) == 1 && rt.segs[0].text == "" {
		rt.segs = nil
	}
	return rt
}

func HttpRoute(method, path, handler Value) Value {
	m := ""
	if d, ok := some(method); ok {
		m = methodName(d)
	}
	return newRoute(m, path.(string), "", false, handler)
}

func removeTrailingSlash(path string) string { return strings.TrimSuffix(path, "/") }

// prefixPath is the path under the prefix.
func prefixPath(path, prefix string) string {
	prefix = removeTrailingSlash(prefix)
	switch path {
	case "*":
		return prefix + "/*"
	case "/":
		return prefix
	}
	return prefix + path
}

func PrefixPath(prefix, path Value) Value { return prefixPath(path.(string), prefix.(string)) }

func PrefixRoute(prefix, r Value) Value {
	rt, p := r.(*route), prefix.(string)
	under := removeTrailingSlash(p)
	if rt.hasPrefix {
		under = prefixPath(rt.prefix, p)
	}
	return newRoute(rt.method, prefixPath(rt.path, p), under, true, rt.handler)
}

func collapseSlashes(path string) string {
	for strings.Contains(path, "//") {
		path = strings.ReplaceAll(path, "//", "/")
	}
	return path
}

// routerConfig is find-my-way's options as Effect gives them: trailing and
// duplicate slashes ignored, case ignored, parameters of 100 characters.
type routerConfig struct {
	trailing, duplicates, caseSensitive bool
	maxParam                            int32
}

func configOf(v Value) routerConfig {
	o := record(v)
	flag := func(name string, otherwise bool) bool {
		if b, ok := some(o.Get(name)); ok {
			return b.(bool)
		}
		return otherwise
	}
	c := routerConfig{flag("ignoreTrailingSlash", true), flag("ignoreDuplicateSlashes", true), flag("caseSensitive", false), 100}
	if n, ok := some(o.Get("maxParamLength")); ok {
		c.maxParam = n.(int32)
	}
	return c
}

// pathSegments are the segments of the path of a request URL, as
// find-my-way normalizes it.
func (c routerConfig) pathSegments(url string) []string {
	if !strings.HasPrefix(url, "/") {
		url = removeHost(url)
	}
	if i := strings.IndexAny(url, "?;#"); i >= 0 {
		url = url[:i]
	}
	if c.duplicates {
		url = collapseSlashes(url)
	}
	if c.trailing && len(url) > 1 && strings.HasSuffix(url, "/") {
		url = url[:len(url)-1]
	}
	return strings.Split(url[1:], "/")
}

// decoded is a segment percent-decoded, or as it is if it cannot be.
func decoded(s string) string {
	if d, ok := decodeURIComponent(s); ok {
		return d
	}
	return s
}

// matches is whether the route matches the method and the segments.
func (rt *route) matches(c routerConfig, method string, path []string) bool {
	if rt.method != "" && rt.method != method {
		return false
	}
	segs := rt.segs
	if c.trailing && len(segs) > 1 && segs[len(segs)-1].text == "" && segs[len(segs)-1].kind == segStatic {
		segs = segs[:len(segs)-1]
	}
	if len(path) < len(segs) || len(path) > len(segs) && !rt.wildcard {
		return false
	}
	for i, s := range segs {
		p := path[i]
		switch s.kind {
		case segParam:
			if p == "" || Length(decoded(p)).(int32) > c.maxParam {
				return false
			}
		default:
			if !c.same(s.text, p) && !c.same(s.text, decoded(p)) {
				return false
			}
		}
	}
	return true
}

func (c routerConfig) same(a, b string) bool {
	if c.caseSensitive {
		return a == b
	}
	return strings.EqualFold(a, b)
}

// before is whether `rt` takes the path before `other`, both matching it:
// at the first segment of another kind, the static or parametric one.
func (rt *route) before(other *route) bool {
	for i := 0; ; i++ {
		a, b := rt.kindAt(i), other.kindAt(i)
		if a != b || a == segWildcard || a < segStatic {
			return a < b
		}
	}
}

func (rt *route) kindAt(i int) int {
	if i < len(rt.segs) {
		return rt.segs[i].kind
	}
	if rt.wildcard {
		return segWildcard
	}
	return segStatic - 1
}

// params are the route's parameters of the path, decoded.
func (rt *route) params(path []string) []Value {
	out := []Value{}
	for i, s := range rt.segs {
		if s.kind == segParam {
			out = append(out, []Value{s.text, decoded(path[i])})
		}
	}
	if rt.wildcard && len(path) > len(rt.segs) {
		out = append(out, []Value{"*", decoded(strings.Join(path[len(rt.segs):], "/"))})
	}
	return out
}

func find(c routerConfig, routes []Value, method string, path []string) *route {
	var best *route
	for _, r := range routes {
		rt := r.(*route)
		if rt.matches(c, method, path) && (best == nil || rt.before(best)) {
			best = rt
		}
	}
	return best
}

// HttpRouter handles a request by its routes, a `HEAD` by the `GET` route
// when it has none of its own; a route's handler gets the parameters as
// the service RouteContext.
func HttpRouter(fb *Fiber, config, routes, request Value) Value {
	q, c := requestOf(request), configOf(config)
	path := c.pathSegments(q.url)
	best := find(c, List(routes), q.method, path)
	if best == nil && q.method == "HEAD" {
		best = find(c, List(routes), "GET", path)
	}
	if best == nil {
		return Fail(NewData(serverErrorTag, NewData(routeMissingTag, q.details(None))))
	}
	params := best.params(path)
	if best.hasPrefix {
		q = q.modified()
		q.url = sliceUrl(q.url, prefixSegments(best.prefix, c.duplicates), c.duplicates)
	}
	context := Rec("params", Fn(func(Value) Value { return Succeed(params) }))
	return withEnv(fb, routeContextKey, context, Apply(best.handler, q))
}

// prefixSegments is how many segments of a URL the prefix takes.
func prefixSegments(prefix string, duplicates bool) int {
	n := 0
	for i := 0; i < len(prefix); i++ {
		if prefix[i] == '/' && (!duplicates || i == 0 || prefix[i-1] != '/') {
			n++
		}
	}
	return n
}

// sliceUrl is the URL without its first `n` segments.
func sliceUrl(url string, n int, duplicates bool) string {
	i := 0
	for ; n > 0; n-- {
		if duplicates {
			for i < len(url) && url[i] == '/' {
				i++
			}
		} else if i < len(url) && url[i] == '/' {
			i++
		}
		for i < len(url) && !strings.ContainsRune("/?;#", rune(url[i])) {
			i++
		}
	}
	if i < len(url) {
		return url[i:]
	}
	return "/"
}

func routeParams(fb *Fiber) []Value {
	impl, ok := fb.env.get(routeContextKey)
	if !ok {
		panic(Defect{"Service not found: " + routeContextKey})
	}
	return List(Run(fb, Apply(record(impl).Get("params"), Unit)))
}

func RouterParamsUnknown(pairs Value) Value {
	ps := List(pairs)
	out := newRecord(len(ps))
	for _, p := range ps {
		k, v := pairOf(p)
		out.put(k, v)
	}
	return out
}

// RouterSchemaParams decodes the parameters of the query and of the path,
// the path's over the query's.
func RouterSchemaParams(fb *Fiber, c, request Value) Value {
	in := paramsRecord(searchParamsOf(requestOf(request).url))
	for _, p := range routeParams(fb) {
		k, v := pairOf(p)
		in.put(k, v)
	}
	return CodecDecode(fb, c, in)
}

// requestInput is what the router's decoders read of a request.
func requestInput(fb *Fiber, r Value, body Value) *Record {
	q := requestOf(r)
	in := newRecord(7)
	in.put("method", q.method)
	in.put("url", q.url)
	in.put("cookies", q.cookieValues())
	in.put("headers", q.headerRecord())
	in.put("pathParams", RouterParamsUnknown(routeParams(fb)))
	in.put("searchParams", paramsRecord(searchParamsOf(q.url)))
	if body != nil {
		in.put("body", body)
	}
	return in
}

func RouterSchemaNoBody(fb *Fiber, c, request Value) Value {
	return CodecDecode(fb, c, requestInput(fb, request, nil))
}

func RouterSchemaJson(fb *Fiber, c, request Value) Value {
	return CodecDecode(fb, c, requestInput(fb, request, RequestJson(fb, request)))
}
