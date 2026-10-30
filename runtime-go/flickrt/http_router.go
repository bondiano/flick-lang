package flickrt

import "strings"

// Http.Router as Effect's HttpRouter over find-my-way: a request goes to
// the route whose pattern matches its path, a static segment before a
// parameter before a wildcard, segment by segment, the first route of
// equal ones; a route under a prefix sees the URL without it.

var (
	routeContextKey = "Http.Router.RouteContext"
	routeMissingTag = Tag("Http.ServerError.ServerErrorReason.RouteNotFound")
)

const (
	segStatic = iota
	segConstrained
	segParam
	segWildcard
)

type segment struct {
	kind    int
	text    string
	pattern *routePattern
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
	pattern := path
	if pattern == "*" {
		pattern = "/*"
	}
	if strings.HasSuffix(pattern, "/*") {
		rt.wildcard, pattern = true, pattern[:len(pattern)-2]
	}
	parts := patternSegments(strings.TrimPrefix(pattern, "/"))
	for i, p := range parts {
		if p == "" && i < len(parts)-1 {
			continue
		}
		rt.segs = append(rt.segs, routeSegment(p))
	}
	if rt.wildcard && len(rt.segs) == 1 && rt.segs[0].kind == segStatic && rt.segs[0].text == "" {
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

// match checks the constraints and extracts parameters in the same pass.
func (rt *route) match(c routerConfig, method string, path []string, params []routeParam) ([]routeParam, bool) {
	if rt.method != "" && rt.method != method {
		return nil, false
	}
	segs := rt.segs
	if c.trailing && len(segs) > 1 && segs[len(segs)-1].text == "" && segs[len(segs)-1].kind == segStatic {
		segs = segs[:len(segs)-1]
	}
	if len(path) < len(segs) || len(path) > len(segs) && !rt.wildcard {
		return nil, false
	}
	for i, s := range segs {
		p := path[i]
		switch s.kind {
		case segParam:
			v := decoded(p)
			if p == "" || Length(v).(int32) > c.maxParam {
				return nil, false
			}
			params = append(params, routeParam{s.text, v})
		case segConstrained:
			v := decoded(p)
			var ok bool
			params, ok = s.pattern.match(c, v, params)
			if !ok {
				return nil, false
			}
		default:
			if !c.same(s.text, p) && !c.same(s.text, decoded(p)) {
				return nil, false
			}
		}
	}
	if rt.wildcard && len(path) > len(segs) {
		params = append(params, routeParam{"*", decoded(strings.Join(path[len(segs):], "/"))})
	}
	return params, true
}

func (c routerConfig) same(a, b string) bool {
	if c.caseSensitive {
		return a == b
	}
	return strings.EqualFold(a, b)
}

// before is whether `rt` takes the path before `other`, both matching it:
// at the first segment of another kind, the static or parametric one.
func (rt *route) before(other *route, c routerConfig, routes []Value, method string) bool {
	for i := 0; ; i++ {
		a, b := rt.kindAt(i), other.kindAt(i)
		if a != b || a == segWildcard || a < segStatic {
			return a < b
		}
		if a == segConstrained {
			left, right := rt.segs[i].pattern, other.segs[i].pattern
			if len(left.prefix) != len(right.prefix) {
				return len(left.prefix) > len(right.prefix)
			}
			if left.constrained != right.constrained {
				return left.constrained
			}
			if left.suffix != right.suffix {
				if left.suffix != "" && (right.suffix == "" || strings.HasSuffix(left.suffix, right.suffix)) {
					return true
				}
				if right.suffix != "" && (left.suffix == "" || strings.HasSuffix(right.suffix, left.suffix)) {
					return false
				}
			}
			if !left.sameMatcher(right) {
				// Distinct equal-priority parameter nodes keep creation order,
				// even when their first registered route does not match.
				for _, value := range routes {
					registered := value.(*route)
					if registered.method != "" && registered.method != method {
						continue
					}
					if registered.sharesNodes(c, rt, i) {
						return true
					}
					if registered.sharesNodes(c, other, i) {
						return false
					}
				}
			}
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
func (rt *route) sharesNodes(c routerConfig, other *route, through int) bool {
	if len(rt.segs) <= through {
		return false
	}
	for i := range through + 1 {
		left, right := rt.segs[i], other.segs[i]
		if left.kind != right.kind {
			return false
		}
		if left.kind == segStatic && !c.same(left.text, right.text) {
			return false
		}
		if left.kind == segConstrained && (!c.same(left.pattern.prefix, right.pattern.prefix) || !left.pattern.sameMatcher(right.pattern)) {
			return false
		}
	}
	return true
}

func find(c routerConfig, routes []Value, method string, path []string) (*route, []Value) {
	var best *route
	var first, second [8]routeParam
	scratch, selected := first[:0], second[:0]
	for _, r := range routes {
		rt := r.(*route)
		if best != nil && !rt.before(best, c, routes, method) {
			continue
		}
		if values, ok := rt.match(c, method, path, scratch[:0]); ok {
			best, selected, scratch = rt, values, selected[:0]
		}
	}
	params := make([]Value, len(selected))
	for i, p := range selected {
		params[i] = []Value{p.name, p.value}
	}
	return best, params
}

// HttpRouter handles a request by its routes, a `HEAD` by the `GET` route
// when it has none of its own; a route's handler gets the parameters as
// the service RouteContext.
func HttpRouter(fb *Fiber, config, routes, request Value) Value {
	q, c := requestOf(request), configOf(config)
	path := c.pathSegments(q.url)
	best, params := find(c, List(routes), q.method, path)
	if best == nil && q.method == "HEAD" {
		best, params = find(c, List(routes), "GET", path)
	}
	if best == nil {
		return Fail(NewData(serverErrorTag, NewData(routeMissingTag, q.details(fb, None))))
	}
	if best.hasPrefix {
		q = q.modified()
		q.url = sliceUrl(q.url, prefixSegments(best.prefix, c.duplicates), c.duplicates)
	}
	context := Rec("params", Fn(func(*Fiber, Value) Value { return Succeed(params) }))
	return withEnv(fb, routeContextKey, context, Apply(fb, best.handler, q))
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
	return List(Run(fb, Apply(fb, record(impl).Get("params"), Unit)))
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
