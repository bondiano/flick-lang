package flickrt

import "strings"

// HttpApi servers as the JS runtime's: each endpoint is a route of the
// HTTP router whose request is decoded with the endpoint's codecs (a
// failure is 400 with the issues, never the input), whose middleware and
// handler run with the services where the API was served, and whose typed
// error is sent with its declared status. `listen` serves the routes and
// the OpenAPI document; `route` is one route of a program's own router.

type apiServer struct{ port float64 }

func HttpApiPort(server Value) Value { return server.(*apiServer).port }

// apiRouterConfig is the router's defaults.
var apiRouterConfig = Rec("ignoreTrailingSlash", None, "ignoreDuplicateSlashes", None,
	"caseSensitive", None, "maxParamLength", None)

// typedFailure is the error a cause fails with when it has no defect: what
// a recovery from typed errors sees.
func typedFailure(c *Cause) (Value, bool) {
	if c.has(causeDie) {
		return nil, false
	}
	if r := c.firstOf(causeFail); r != nil {
		return r.err, true
	}
	return nil, false
}

// runMaybe runs the task a function of the program answered; a lifted
// pure function answers its value.
func runMaybe(fb *Fiber, v Value) Value {
	if t, ok := v.(Eff); ok {
		return Run(fb, t)
	}
	return v
}

func apiJSON(status int32, v Value) Value {
	return bodyResponse(noOptions, status, nil, *textBody(JsonStringify(v).(string), "application/json"))
}

func apiEmpty(status int32) Value { return newResponse(status, "", false, nil, nil, nil, false) }

func apiInternal() Value { return apiJSON(500, Rec("error", "InternalServerError")) }

// apiBadRequest is 400 with the issues of a request that does not decode,
// or the status of a form's MultipartError.
func apiBadRequest(err Value) Value {
	d := err.(*Data)
	if d.Tag == multipartErrorTag {
		return apiEmpty(multipartStatus[d.Fields()[0].(*Data).Tag])
	}
	issues := List(d.Fields()[0])
	out := make([]Value, len(issues))
	for i, is := range issues {
		r := record(is)
		out[i] = Rec("path", r.Get("path"), "message", r.Get("message"))
	}
	return apiJSON(400, Rec("error", "BadRequest", "issues", out))
}

// apiDecode decodes a part, failing with the SchemaError; Unit without a codec.
func apiDecode(fb *Fiber, c codec, in Value) Value {
	if c == nil {
		return Unit
	}
	v, issues := c.decode(fb, in)
	if issues != nil {
		return Fail(schemaErrorOf(issues))
	}
	return v
}

// withoutExtra is the object without the keys a record codec does not
// name: a request has more headers than any endpoint declares.
func withoutExtra(c codec, in *Record) Value {
	if x, ok := c.(*wire); ok {
		if s := x.s.resolved(); s.kind == kindRecord {
			out := newRecord(len(s.labels))
			for i, k := range in.keys {
				for _, l := range s.labels {
					if l == k {
						out.put(k, in.vals[i])
					}
				}
			}
			return out
		}
	}
	return in
}

// apiBody is the body in the endpoint's encoding: JSON (text that is no
// JSON stays a string, which a record codec refuses), the text itself, or
// the fields of a URL-encoded form.
func apiBody(fb *Fiber, q *ServerRequest, encoding string) Value {
	text, c := catch(func() Value { return decodeUTF8(q.bytes(fb)) })
	if c != nil {
		if _, ok := typedFailure(c); !ok {
			panic(&Failure{c})
		}
		text = ""
	}
	switch encoding {
	case encText:
		return text
	case encForm:
		out := newRecord(4)
		for _, p := range formDecode(text.(string)) {
			k, v := pairOf(p)
			out.put(k, v)
		}
		return out
	}
	if v, reason := parseJSON(text.(string)); reason == "" {
		return v
	}
	return text
}

// apiAuthorized is whether the request carries the credentials of one of the schemes.
func apiAuthorized(schemes []*apiSecurity, q *ServerRequest) bool {
	if len(schemes) == 0 {
		return true
	}
	authorization, _ := q.header("authorization")
	authorization = ToLowerCase(authorization).(string)
	cookies, _ := q.header("cookie")
	for _, s := range schemes {
		switch {
		case s.kind == "bearer":
			if strings.HasPrefix(authorization, "bearer ") {
				return true
			}
		case s.kind == "basic":
			if strings.HasPrefix(authorization, "basic ") {
				return true
			}
		case s.in == "query":
			for _, p := range searchParamsOf(q.url) {
				if k, _ := pairOf(p); k == s.key {
					return true
				}
			}
		case s.in == "cookie":
			for _, c := range strings.Split(cookies, ";") {
				if strings.HasPrefix(strings.TrimSpace(c), s.key+"=") {
					return true
				}
			}
		default:
			if _, ok := q.header(ToLowerCase(s.key).(string)); ok {
				return true
			}
		}
	}
	return false
}

// apiRequest decodes the request's parts, failing with the SchemaError or
// the form's MultipartError.
func apiRequest(fb *Fiber, e *apiEndpoint, r Value) Value {
	q := requestOf(r)
	var payload Value = Unit
	switch e.payloadEncoding {
	case encBytes:
		got, c := catch(func() Value { return byteList(q.bytes(fb)) })
		if c != nil {
			if _, ok := typedFailure(c); !ok {
				panic(&Failure{c})
			}
			got = []Value{}
		}
		payload = got
	case encParts:
		payload = MultipartParts(r)
	}
	params := RouterParamsUnknown(routeParams(fb)).(*Record)
	query := newRecord(4)
	for _, p := range searchParamsOf(q.url) {
		k, v := pairOf(p)
		query.put(k, v)
	}
	path := apiDecode(fb, e.params, params)
	decodedQuery := apiDecode(fb, e.query, query)
	headers := Unit
	if e.headers != nil {
		headers = apiDecode(fb, &ignoring{e.headers}, q.headerRecord())
	}
	switch {
	case e.payloadEncoding == encMultipart:
		payload = apiDecode(fb, e.payload, q.persisted(fb))
	case e.hasPayload() && e.payloadEncoding != encBytes && e.payloadEncoding != encParts:
		payload = apiDecode(fb, e.payload, apiBody(fb, q, e.payloadEncoding))
	}
	return Rec("path", path, "query", decodedQuery, "headers", headers, "payload", payload)
}

// serveEndpoint answers a request of the endpoint, in a scope of its own
// that removes the files of its form.
func serveEndpoint(fb *Fiber, e *apiEndpoint, handler Value, services *env, r Value) Value {
	// HEAD has no route of its own: it is served by the route of every method of its path
	if e.method == "HEAD" && requestOf(r).method != "HEAD" {
		return apiEmpty(405)
	}
	if !apiAuthorized(e.security, requestOf(r)) {
		return apiJSON(401, Rec("error", "Unauthorized"))
	}
	return owned(fb, func(fb *Fiber, _ *Owner) Value {
		request, c := catch(func() Value { return apiRequest(fb, e, r) })
		if c != nil {
			if err, ok := typedFailure(c); ok {
				return apiBadRequest(err)
			}
			panic(&Failure{c})
		}
		inner := *fb
		inner.env = services
		value, c := catch(func() Value {
			for _, hook := range e.middleware {
				runMaybe(&inner, Apply(&inner, hook, request))
			}
			serving := inner
			if a := e.auth; a != nil {
				// each request makes its own service: concurrent requests do not share it
				serving.env = inner.env.with(a.tag, runMaybe(&inner, Apply(&inner, a.check, request)))
			}
			return runMaybe(&serving, Apply(&serving, handler, request))
		})
		if c != nil {
			if err, ok := typedFailure(c); ok {
				return apiFailed(fb, e, err)
			}
			panic(&Failure{c})
		}
		return apiSucceeded(fb, e, value)
	})
}

// codecHas is whether the value is one of the codec's type: a variant of
// its type, or a value it encodes.
func codecHas(fb *Fiber, c codec, v Value) (has bool) {
	if s := codecType(c); s != nil && s.kind == kindAdt {
		d, ok := v.(Constructed)
		if !ok {
			return false
		}
		found := false
		for _, variant := range s.variants {
			found = found || variant.Tag == d.TagName()
		}
		if !found {
			return false
		}
	}
	defer func() {
		if recover() != nil {
			has = false
		}
	}()
	_, issues := c.encode(fb, v)
	return issues == nil
}

// codecType is the schema of a codec's values, when it has one.
func codecType(c codec) *Schema {
	switch t := c.(type) {
	case *wire:
		return t.s.resolved()
	case *transformed:
		if t.typed != nil {
			return t.typed.resolved()
		}
	}
	return nil
}

// codecOpaque is whether a codec takes any value: a transformation without
// the schema of its values cannot tell one error from another.
func codecOpaque(c codec) bool {
	t, ok := c.(*transformed)
	return ok && t.typed == nil
}

// apiFailed is the response of a typed error: the first declared case
// whose codec has the value, with its status; an HttpError always with its
// own status, as `{ message }` when no case declares it; a streamed form's
// MultipartError with the status it names; anything else 500.
func apiFailed(fb *Fiber, e *apiEndpoint, err Value) Value {
	var found *apiErrorCase
	for i := range e.errors {
		if codecHas(fb, e.errors[i].codec, err) {
			found = &e.errors[i]
			break
		}
	}
	d, _ := err.(*Data)
	var own *Record
	if d != nil && d.Tag == httpErrorTag {
		own = record(d.Fields()[0])
	}
	if found == nil {
		switch {
		case own != nil:
			return apiJSON(own.Get("status").(int32), Rec("message", own.Get("message")))
		case d != nil && d.Tag == multipartErrorTag:
			return apiBadRequest(err)
		}
		logMessage(fb, "Error", e.name+": an undeclared error")
		return apiInternal()
	}
	encoded, issues := found.codec.encode(fb, err)
	if issues != nil {
		return apiInternal()
	}
	status := found.status
	if own != nil {
		status = own.Get("status").(int32)
	}
	return apiJSON(status, encoded)
}

func apiSuccessBody(fb *Fiber, e *apiEndpoint, value Value) Value {
	if other := apiOtherSuccess(e, value); other != nil {
		return other
	}
	if e.success == nil {
		return apiEmpty(e.status)
	}
	encoded, issues := e.success.encode(fb, value)
	if issues != nil {
		return Fail(schemaErrorOf(issues))
	}
	if e.successEncoding == encText {
		return bodyResponse(noOptions, e.status, nil, *textBody(jsString(encoded), "text/plain"))
	}
	return apiJSON(e.status, encoded)
}

// apiSucceeded is the response of the handler's value; one that cannot be
// written is 500.
func apiSucceeded(fb *Fiber, e *apiEndpoint, value Value) Value {
	out, c := catch(func() Value {
		inner, extras := apiUnwrapExtras(fb, e.extras, value)
		return extras(apiSuccessBody(fb, e, inner))
	})
	if c != nil {
		if _, ok := typedFailure(c); ok {
			return apiInternal()
		}
		panic(&Failure{c})
	}
	return out
}

// apiRoutes is a route of the router per endpoint, whose handler runs with
// the services of `fb`.
func apiRoutes(fb *Fiber, a *apiDefinition, handlers Value) []Value {
	services := fb.env
	all := record(handlers)
	var routes []Value
	for _, g := range a.groups {
		for _, e := range g.endpoints {
			var handler Value
			if group, ok := recordField(all, g.name).(*Record); ok {
				handler = recordField(group, e.name)
			}
			if handler == nil {
				panic(Defect{"TypeError: no handler for " + g.name + "." + e.name})
			}
			method := e.method
			if method == "HEAD" {
				method = ""
			}
			endpoint := e
			serve := Fn(func(_ *Fiber, r Value) Value {
				return Eff(func(fb *Fiber) Value { return serveEndpoint(fb, endpoint, handler, services, r) })
			})
			routes = append(routes, newRoute(method, routePath(e.path), "", false, serve))
		}
	}
	return routes
}

// routePath is a route's path, which starts with `/` as the router needs.
func routePath(path string) string {
	if !strings.HasPrefix(path, "/") {
		panic(Defect{`TypeError: an HTTP path starts with "/": ` + path})
	}
	return path
}

func constantRoute(path string, response Value) Value {
	return newRoute("GET", routePath(path), "", false, Fn(func(*Fiber, Value) Value { return Succeed(response) }))
}

// HttpApiListen serves the API until the innermost scope closes, with its
// OpenAPI document, Swagger UI and Scalar where the options say.
func HttpApiListen(fb *Fiber, options, api, handlers Value) Value {
	o, a := record(options), api.(*apiDefinition)
	routes := apiRoutes(fb, a, handlers)
	docs := isSome(o.Get("openapi")) || isSome(o.Get("swagger")) || isSome(o.Get("scalar"))
	text := ""
	if docs {
		spec := HttpApiOpenApi(api)
		if !isOk(spec) {
			return Fail(NewData(apiServeTag, value(spec).(*Data).Fields()[0]))
		}
		text = value(spec).(string)
	}
	if p, ok := some(o.Get("openapi")); ok {
		routes = append(routes, constantRoute(p.(string),
			bodyResponse(noOptions, 200, nil, *textBody(text, "application/json"))))
	}
	if p, ok := some(o.Get("swagger")); ok {
		routes = append(routes, constantRoute(p.(string), ResponseHtml(swaggerPage(a.name, text))))
	}
	if p, ok := some(o.Get("scalar")); ok {
		routes = append(routes, constantRoute(p.(string), ResponseHtml(scalarPage(a.name, text))))
	}
	router := Fn(func(_ *Fiber, r Value) Value {
		return Eff(func(fb *Fiber) Value { return HttpRouter(fb, apiRouterConfig, routes, r) })
	})
	serve := Rec("host", o.Get("host"), "port", o.Get("port"), "gracefulShutdownTimeout", None)
	server, c := catch(func() Value { return HttpServe(fb, serve, router) })
	if c != nil {
		if err, ok := typedFailure(c); ok && err.(*Data).Tag == serveErrorTag {
			return Fail(NewData(apiServeTag, err.(*Data).Fields()[0]))
		}
		panic(&Failure{c})
	}
	return &apiServer{HttpAddress(server).(*Data).Fields()[1].(float64)}
}

// HttpApiRoute is one route of every method at `path` that serves every
// endpoint; a request no endpoint takes is 404.
func HttpApiRoute(fb *Fiber, path, api, handlers Value) Value {
	routes := apiRoutes(fb, api.(*apiDefinition), handlers)
	served := Fn(func(_ *Fiber, r Value) Value {
		return Eff(func(fb *Fiber) Value {
			out, c := catch(func() Value { return HttpRouter(fb, apiRouterConfig, routes, r) })
			if c != nil {
				if _, ok := typedFailure(c); ok {
					return apiEmpty(404)
				}
				panic(&Failure{c})
			}
			return out
		})
	})
	return newRoute("", routePath(path.(string)), "", false, served)
}

// The documentation pages load Swagger UI and Scalar from a CDN.
//
// shortcut: the JS runtime inlines bundles shipped with its dependencies, which the go target has no copy of; embed them if pages must work offline
func escapeJSON(text string) string { return strings.ReplaceAll(text, "<", "\\u003c") }

func swaggerPage(title, spec string) string {
	return `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8" /><title>` + title +
		`</title><link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui.css" /></head><body><div id="swagger-ui"></div><script id="swagger-spec" type="application/json">` +
		escapeJSON(spec) + `</script><script src="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui-bundle.js"></script><script>
window.onload = () => { window.ui = SwaggerUIBundle({ spec: JSON.parse(document.getElementById("swagger-spec").textContent), dom_id: "#swagger-ui" }); };</script></body></html>`
}

func scalarPage(title, spec string) string {
	return `<!doctype html><html><head><meta charset="utf-8" /><title>` + title +
		`</title></head><body><div id="api-reference-container"></div><script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.25.11"></script><script>window.Scalar.createApiReference(document.getElementById("api-reference-container"), { _integration: "html", content: ` +
		escapeJSON(spec) + ` })</script></body></html>`
}
