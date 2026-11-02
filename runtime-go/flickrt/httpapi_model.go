package flickrt

import "strings"

// HttpApi values as the JS runtime's: endpoints, groups and APIs are
// immutable data that the server, the client and the OpenAPI writer read.
// A label is the name the compiler passes for `Record.Label`; the codecs
// are those of the endpoint's parts, nil for a part it does not declare.

var (
	httpErrorTag      = Tag("HttpApi.HttpError.HttpError")
	apiServeTag       = Tag("HttpApi.ServeError.ServeError")
	apiClientErrorTag = Tag("HttpApi.ClientError.ClientError")
)

// How a body is written: its encoding names its media type.
const (
	encJSON      = "json"
	encText      = "text"
	encForm      = "form"
	encMultipart = "multipart"
	encBytes     = "bytes"
	encParts     = "parts"
	encStream    = "stream"
	encSSE       = "sse"
	encEvents    = "events"
)

// apiSecurity is `HttpApi.Security`: a bearer token, a user and password,
// or a key in a header, the query or a cookie (`in`).
type apiSecurity struct {
	kind, name, key, in string
}

// apiErrorCase is one of `HttpApi.ErrorCases`: a typed error's status and codec.
type apiErrorCase struct {
	status int32
	codec  codec
}

// apiErrors is `HttpApi.ErrorCases e`.
type apiErrors []apiErrorCase

// apiExtra is what a handler answers besides its value: the response
// headers of a codec, or the value of the cookie `key`.
type apiExtra struct {
	headers codec
	cookie  string
}

type apiEndpoint struct {
	name, method, path     string
	params, query, headers codec
	payload                codec
	payloadEncoding        string
	status                 int32
	successEncoding        string
	success                codec
	extras                 []apiExtra // innermost first
	errors                 apiErrors
	description            string
	security               []*apiSecurity
	middleware             []Value // run in order before the handler
	auth                   *apiAuth
	deprecated, hidden     bool
}

// apiAuth is `HttpApi.with_auth`: the service `tag` that `check` makes from
// each request, for the handler; `optional` serves anonymous requests too.
type apiAuth struct {
	security *apiSecurity
	tag      string
	check    Value
	optional bool
}

type apiGroup struct {
	name      string
	endpoints []*apiEndpoint
}

type apiInfo struct {
	version, description string
	servers              []Value
}

type apiDefinition struct {
	name   string
	info   apiInfo
	groups []*apiGroup
}

func endpointOf(v Value) *apiEndpoint { return v.(*apiEndpoint) }

// hasPayload is whether the endpoint reads a body: one of a codec, or bytes or parts.
func (e *apiEndpoint) hasPayload() bool {
	return e.payload != nil || e.payloadEncoding == encBytes || e.payloadEncoding == encParts
}

// hasSuccessBody is whether the success has a body: one of a codec, or
// bytes, a stream or events.
func (e *apiEndpoint) hasSuccessBody() bool {
	switch e.successEncoding {
	case encBytes, encStream, encEvents:
		return true
	}
	return e.success != nil
}

func optionalCodec(c Value) codec {
	if c == nil {
		return nil
	}
	return codecOf(c)
}

var apiMethods = map[string]string{
	"Get": "GET", "Post": "POST", "Put": "PUT", "Patch": "PATCH",
	"Delete": "DELETE", "Head": "HEAD", "Options": "OPTIONS", "Query": "QUERY",
}

func HttpApiEndpoint(method, path, name Value) Value {
	m, ok := apiMethods[ConstructorName(method.(*Data).Tag)]
	if !ok {
		panic(Defect{"TypeError: not an HTTP method: " + method.(*Data).Tag})
	}
	return &apiEndpoint{name: name.(string), method: m, path: path.(string), payloadEncoding: encJSON,
		status: 204, successEncoding: encJSON}
}

// changed is a copy of the endpoint changed by `f`.
func changedEndpoint(e Value, f func(*apiEndpoint)) Value {
	c := *endpointOf(e)
	f(&c)
	return &c
}

func HttpApiWithPath(c, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) { x.params = codecOf(c) })
}

func HttpApiWithQuery(c, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) { x.query = codecOf(c) })
}

func HttpApiWithHeaders(c, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) { x.headers = codecOf(c) })
}

func withPayload(c Value, encoding string, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) { x.payload, x.payloadEncoding = optionalCodec(c), encoding })
}

func HttpApiWithPayload(c, e Value) Value          { return withPayload(c, encJSON, e) }
func HttpApiWithTextPayload(c, e Value) Value      { return withPayload(c, encText, e) }
func HttpApiWithFormPayload(c, e Value) Value      { return withPayload(c, encForm, e) }
func HttpApiWithMultipartPayload(c, e Value) Value { return withPayload(c, encMultipart, e) }
func HttpApiWithBytesPayload(e Value) Value        { return withPayload(nil, encBytes, e) }
func HttpApiWithMultipartStreamPayload(e Value) Value {
	return withPayload(nil, encParts, e)
}

func withSuccess(status Value, c Value, encoding string, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) {
		x.status, x.success, x.successEncoding = status.(int32), optionalCodec(c), encoding
	})
}

func HttpApiWithSuccess(status, c, e Value) Value     { return withSuccess(status, c, encJSON, e) }
func HttpApiWithTextSuccess(status, c, e Value) Value { return withSuccess(status, c, encText, e) }
func HttpApiWithSseSuccess(status, c, e Value) Value  { return withSuccess(status, c, encSSE, e) }
func HttpApiWithBytesSuccess(status, e Value) Value   { return withSuccess(status, nil, encBytes, e) }
func HttpApiWithStreamSuccess(status, e Value) Value  { return withSuccess(status, nil, encStream, e) }
func HttpApiWithSseEventsSuccess(status, e Value) Value {
	return withSuccess(status, nil, encEvents, e)
}

func withExtra(extra apiExtra, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) {
		x.extras = append(append([]apiExtra{}, x.extras...), extra)
	})
}

func HttpApiWithSuccessHeaders(c, e Value) Value { return withExtra(apiExtra{headers: codecOf(c)}, e) }

// HttpApiSecuritySetCookie needs an API key in a cookie.
func HttpApiSecuritySetCookie(security, e Value) Value {
	s := security.(*apiSecurity)
	if s.kind != "apiKey" || s.in != "cookie" {
		panic(Defect{"TypeError: security_set_cookie needs an API key in a cookie: " + s.name})
	}
	return withExtra(apiExtra{cookie: s.key}, e)
}

// HttpApiWithErrors is `with_errors cases endpoint`. Of several cases each
// must tell its errors from the others', or a response would take the
// first case's status.
func HttpApiWithErrors(errors, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) {
		x.errors = append(append(apiErrors{}, x.errors...), errors.(apiErrors)...)
		for _, c := range x.errors {
			if len(x.errors) > 1 && codecOpaque(c.codec) {
				panic(Defect{"TypeError: the errors of " + x.name + " cannot be told apart: give the codec of each a type with Codec.transform_as"})
			}
		}
	})
}

func HttpApiDescribe(description, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) { x.description = description.(string) })
}

var HttpApiErrors Value = apiErrors{}

func HttpApiErrorCase(status, c, cases Value) Value {
	return append(append(apiErrors{}, cases.(apiErrors)...), apiErrorCase{status.(int32), codecOf(c)})
}

func HttpApiWithSecurity(security, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) {
		x.security = append(append([]*apiSecurity{}, x.security...), security.(*apiSecurity))
	})
}

func HttpApiWithMiddleware(hook, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) {
		x.middleware = append(append([]Value{}, x.middleware...), hook)
	})
}

// HttpApiWithAuth is `with_auth security tag check endpoint`.
func HttpApiWithAuth(security, tag, check, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) {
		x.auth = &apiAuth{security.(*apiSecurity), tag.(string), check, false}
	})
}

// HttpApiWithOptionalAuth is `with_optional_auth security tag check endpoint`.
func HttpApiWithOptionalAuth(security, tag, check, e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) {
		x.auth = &apiAuth{security.(*apiSecurity), tag.(string), check, true}
	})
}

// HttpApiAuthenticated is `authenticated tag group`: every endpoint makes
// the service `tag` for its handler.
func HttpApiAuthenticated(tag, group Value) Value {
	g := group.(*apiGroup)
	for _, e := range g.endpoints {
		if e.auth == nil || e.auth.tag != tag.(string) {
			panic(Defect{"TypeError: the endpoint " + e.name + " has no `with_auth` making " + tag.(string)})
		}
	}
	return g
}

func HttpApiDeprecate(e Value) Value {
	return changedEndpoint(e, func(x *apiEndpoint) { x.deprecated = true })
}

func HttpApiHide(e Value) Value { return changedEndpoint(e, func(x *apiEndpoint) { x.hidden = true }) }

func HttpApiBearer(name Value) Value { return &apiSecurity{kind: "bearer", name: name.(string)} }
func HttpApiBasic(name Value) Value  { return &apiSecurity{kind: "basic", name: name.(string)} }

// HttpApiKey is `api_key name { key; location }`.
func HttpApiKey(name, options Value) Value {
	o := record(options)
	in := "header"
	switch ConstructorName(o.Get("location").(*Data).Tag) {
	case "InQuery":
		in = "query"
	case "InCookie":
		in = "cookie"
	}
	return &apiSecurity{kind: "apiKey", name: name.(string), key: o.Get("key").(string), in: in}
}

func HttpApiGroup(name Value) Value { return &apiGroup{name: name.(string)} }

func HttpApiAdd(_, _, endpoint, group Value) Value {
	g := group.(*apiGroup)
	return &apiGroup{g.name, append(append([]*apiEndpoint{}, g.endpoints...), endpointOf(endpoint))}
}

func HttpApiMake(name Value) Value {
	return &apiDefinition{name: name.(string), info: apiInfo{version: "0.1.0"}}
}

func HttpApiInfo(info, api Value) Value {
	i, a := record(info), *api.(*apiDefinition)
	a.info = apiInfo{i.Get("version").(string), i.Get("description").(string), List(i.Get("servers"))}
	return &a
}

// HttpApiUnder is `HttpApi.Mount.under prefix api`: every endpoint's path
// after `prefix`.
func HttpApiUnder(prefix, api Value) Value {
	a := *api.(*apiDefinition)
	groups := make([]*apiGroup, len(a.groups))
	for i, g := range a.groups {
		endpoints := make([]*apiEndpoint, len(g.endpoints))
		for j, e := range g.endpoints {
			moved := *e
			moved.path = joinPath(prefix.(string), e.path)
			endpoints[j] = &moved
		}
		groups[i] = &apiGroup{g.name, endpoints}
	}
	a.groups = groups
	return &a
}

// joinPath is `/api` and `/users` as `/api/users`, whatever slashes either
// has.
func joinPath(prefix, path string) string {
	var parts []string
	for _, p := range []string{prefix, path} {
		if p = strings.Trim(p, "/"); p != "" {
			parts = append(parts, p)
		}
	}
	return "/" + strings.Join(parts, "/")
}

func HttpApiAddGroup(_, _, group, api Value) Value {
	a := *api.(*apiDefinition)
	a.groups = append(append([]*apiGroup{}, a.groups...), group.(*apiGroup))
	return &a
}
