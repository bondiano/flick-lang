package flickrt

import (
	"math/big"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
)

// Http.Platform.node and Http.StaticServer, as on Node: a file's response
// with its media type, strong ETag, modification time and the range of it
// asked for, read as it is sent, and a handler of the files below a
// directory.

var internalErrorTag = Tag("Http.ServerError.ServerErrorReason.InternalError")

// fileResponse is the response of the file, from `offset` at most
// `bytesToRead` bytes (-1: to the end), or the PlatformError of its stat.
func fileResponse(path string, o *Record, status int32, offset, bytesToRead int64) (*ServerResponse, Value) {
	fi, err := os.Stat(path)
	if err != nil {
		return nil, systemError(err, "stat", "stat", path)
	}
	size := fi.Size()
	offset = min(offset, size)
	length := size - offset
	if bytesToRead >= 0 && bytesToRead < length {
		length = bytesToRead
	}
	headers := headersOf(HeadersEmpty)
	statusText, hasText := "", false
	if o != nil {
		if h := optionHeaders(o); h != nil {
			headers = h
		}
		if t, ok := some(o.Get("contentType")); ok && t != "" {
			headers = headers.set("content-type", t)
		}
		if s, ok := some(o.Get("status")); ok {
			status = s.(int32)
		}
		if t, ok := some(o.Get("statusText")); ok {
			statusText, hasText = t.(string), true
		}
	}
	headers = headers.set("etag", `"`+etagOf(record(infoOf(fi)))+`"`)
	headers = headers.set("last-modified", fi.ModTime().UTC().Format(http.TimeFormat))
	if _, ok := recordField(headers, "content-type").(string); !ok {
		mime := "application/octet-stream"
		if t, ok := some(MimeGetType(path)); ok {
			mime = t.(string)
		}
		headers = headers.set("content-type", mime)
	}
	headers = headers.set("content-length", strconv.FormatInt(length, 10))
	body := &Body{kind: bodyStream, length: -1, open: fileOpener(path, offset, length)}
	return newResponse(status, statusText, hasText, headers, nil, body, true), nil
}

func optionalSize(v Value) int64 {
	if s, ok := some(v); ok && s.(*big.Int).IsInt64() {
		return s.(*big.Int).Int64()
	}
	return -1
}

// HttpPlatformNode is the layer of the platform's service of key `key`.
func HttpPlatformNode(key Value) Value {
	return LayerSucceed(key, Rec("file_response", Fn(func(fb *Fiber, options Value) Value {
		return Fn(func(fb *Fiber, path Value) Value {
			return Eff(func(*Fiber) Value {
				o := record(options)
				r, failure := fileResponse(path.(string), record(o.Get("response")), 200,
					max(optionalSize(o.Get("offset")), 0), optionalSize(o.Get("bytesToRead")))
				if failure != nil {
					return Fail(failure)
				}
				return r
			})
		})
	})))
}

var staticMimeTypes = map[string]string{
	"html": "text/html; charset=utf-8", "htm": "text/html; charset=utf-8", "css": "text/css; charset=utf-8",
	"js": "text/javascript; charset=utf-8", "mjs": "text/javascript; charset=utf-8",
	"json": "application/json; charset=utf-8", "xml": "application/xml; charset=utf-8",
	"txt": "text/plain; charset=utf-8", "csv": "text/csv; charset=utf-8", "md": "text/markdown; charset=utf-8",
	"yaml": "text/yaml; charset=utf-8", "yml": "text/yaml; charset=utf-8", "png": "image/png",
	"jpg": "image/jpeg", "jpeg": "image/jpeg", "gif": "image/gif", "svg": "image/svg+xml; charset=utf-8",
	"ico": "image/x-icon", "webp": "image/webp", "avif": "image/avif", "woff": "font/woff",
	"woff2": "font/woff2", "ttf": "font/ttf", "otf": "font/otf", "eot": "application/vnd.ms-fontobject",
	"mp3": "audio/mpeg", "mp4": "video/mp4", "webm": "video/webm", "ogg": "audio/ogg", "wav": "audio/wav",
	"flac": "audio/flac", "aac": "audio/aac", "pdf": "application/pdf", "zip": "application/zip",
	"gz": "application/gzip", "wasm": "application/wasm", "map": "application/json",
	"webmanifest": "application/manifest+json",
}

// nodeExtname is Node's `path.extname`: from the last dot of the base name,
// none for a name that only starts with one.
func nodeExtname(path string) string {
	base := filepath.Base(path)
	if i := strings.LastIndexByte(base, '.'); i > 0 {
		return base[i:]
	}
	return ""
}

type staticServer struct {
	root, index, cacheControl string
	hasIndex, spa, hasCache   bool
	mimeTypes                 map[string]string
}

func (s *staticServer) mimeType(path string) string {
	ext := strings.ToLower(nodeExtname(path))
	if len(ext) <= 1 {
		return "application/octet-stream"
	}
	if t, ok := s.mimeTypes[ext[1:]]; ok {
		return t
	}
	if t, ok := staticMimeTypes[ext[1:]]; ok {
		return t
	}
	return "application/octet-stream"
}

func serverFailure(fb *Fiber, q *ServerRequest, reason string) Value {
	return Fail(NewData(serverErrorTag, NewData(reason, q.details(fb, None))))
}

// resolve is the file of the URL below the root, false outside it.
func (s *staticServer) resolve(url string) (string, bool) {
	url, _, _ = strings.Cut(url, "?")
	decoded, ok := decodeURIComponent(url)
	if !ok || strings.ContainsRune(decoded, 0) {
		return "", false
	}
	seps := "/"
	if runtime.GOOS == "windows" {
		seps = `/\`
	}
	rel := strings.TrimLeft(decoded, seps)
	if filepath.IsAbs(rel) || filepath.VolumeName(rel) != "" {
		return "", false
	}
	candidate := filepath.Join(s.root, rel)
	within, err := filepath.Rel(s.root, candidate)
	if err != nil || within == ".." || strings.HasPrefix(within, ".."+string(filepath.Separator)) || filepath.IsAbs(within) {
		return "", false
	}
	return candidate, true
}

// response is the file's response with the server's headers, or a
// failure of the request: not found for a missing file.
func (s *staticServer) response(fb *Fiber, q *ServerRequest, path string, status int32, offset, bytesToRead int64) *ServerResponse {
	r, failure := fileResponse(path, nil, status, offset, bytesToRead)
	if failure != nil {
		if isNotFound(failure) {
			serverFailure(fb, q, routeMissingTag)
		}
		serverFailure(fb, q, internalErrorTag)
	}
	out := ResponseSetHeaders([]Value{[]Value{"Content-Type", s.mimeType(path)}, []Value{"Accept-Ranges", "bytes"}}, r)
	if s.hasCache {
		out = ResponseSetHeader("Cache-Control", s.cacheControl, out)
	}
	return responseOf(out)
}

func responseHeader(r *ServerResponse, name string) (string, bool) {
	v, ok := recordField(r.headerRecord(), name).(string)
	return v, ok
}

func stripWeak(v string) string {
	v = strings.TrimSpace(v)
	if len(v) >= 2 && (v[0] == 'W' || v[0] == 'w') && v[1] == '/' {
		return v[2:]
	}
	return v
}

// notModified is the answer of a conditional request the file still
// matches, or nil.
func notModified(q *ServerRequest, full *ServerResponse) *ServerResponse {
	etag, hasEtag := responseHeader(full, "etag")
	matches := false
	if inm, ok := q.header("if-none-match"); ok {
		for _, c := range strings.Split(inm, ",") {
			c = strings.TrimSpace(c)
			if c == "*" || c != "" && hasEtag && stripWeak(c) == stripWeak(etag) {
				matches = true
				break
			}
		}
	} else if ims, ok := q.header("if-modified-since"); ok {
		lm, hasLm := responseHeader(full, "last-modified")
		// NaN compares false
		matches = hasLm && parseJsDate(lm) <= parseJsDate(ims)
	}
	if !matches {
		return nil
	}
	h := headersOf(HeadersEmpty)
	for _, name := range []string{"etag", "cache-control", "last-modified"} {
		if v, ok := responseHeader(full, name); ok {
			h = h.set(name, v)
		}
	}
	return newResponse(304, "", false, h, nil, nil, false)
}

// parseRange is a single byte range of the file: its first and last byte,
// unsatisfiable, or ok false for a header to ignore.
func parseRange(header string, size int64) (start, end int64, unsatisfiable, ok bool) {
	v := strings.TrimSpace(header)
	if !strings.HasPrefix(strings.ToLower(v), "bytes=") {
		return
	}
	v = strings.TrimSpace(v[6:])
	from, to, found := strings.Cut(v, "-")
	from, to = strings.TrimSpace(from), strings.TrimSpace(to)
	if v == "" || strings.Contains(v, ",") || !found || from == "" && to == "" {
		return
	}
	number := func(s string) (int64, bool) {
		if s == "" || strings.Trim(s, "0123456789") != "" {
			return 0, false
		}
		n, err := strconv.ParseInt(s, 10, 64)
		return n, err == nil
	}
	if from == "" {
		suffix, okSuffix := number(to)
		if !okSuffix {
			return
		}
		if suffix == 0 || size == 0 {
			return 0, 0, true, true
		}
		return max(size-suffix, 0), size - 1, false, true
	}
	start, okStart := number(from)
	if !okStart {
		return
	}
	if to == "" {
		return start, size - 1, start >= size, true
	}
	end, okEnd := number(to)
	if !okEnd {
		return
	}
	if start > end || start >= size {
		return 0, 0, true, true
	}
	return start, min(end, size-1), false, true
}

// serve is the response of the file: whole, not modified, or the range
// the request asks for.
func (s *staticServer) serve(fb *Fiber, q *ServerRequest, path string, size int64) *ServerResponse {
	var full *ServerResponse
	whole := func() *ServerResponse {
		if full == nil {
			full = s.response(fb, q, path, 200, 0, -1)
		}
		return full
	}
	_, inm := q.header("if-none-match")
	_, ims := q.header("if-modified-since")
	if inm || ims {
		if r := notModified(q, whole()); r != nil {
			return r
		}
	}
	rangeHeader, hasRange := q.header("range")
	if !hasRange || q.method != "GET" {
		return whole()
	}
	if ifRange, ok := q.header("if-range"); ok {
		// only a strong tag matches, never a date
		v := strings.TrimSpace(ifRange)
		if etag, _ := responseHeader(whole(), "etag"); !strings.HasPrefix(v, `"`) || v != etag {
			return full
		}
	}
	if size < 0 {
		fi, err := os.Stat(path)
		if err != nil {
			if isNotFound(systemError(err, "stat", "stat", path)) {
				serverFailure(fb, q, routeMissingTag)
			}
			serverFailure(fb, q, internalErrorTag)
		}
		size = fi.Size()
	}
	start, end, unsatisfiable, ok := parseRange(rangeHeader, size)
	switch {
	case !ok:
		return whole()
	case unsatisfiable:
		h := headersOf(HeadersEmpty).set("content-range", "bytes */"+strconv.FormatInt(size, 10))
		return newResponse(416, "", false, h, nil, nil, false)
	}
	r := s.response(fb, q, path, 206, start, end-start+1)
	return responseOf(ResponseSetHeader("Content-Range", "bytes "+strconv.FormatInt(start, 10)+"-"+strconv.FormatInt(end, 10)+"/"+strconv.FormatInt(size, 10), r))
}

func (s *staticServer) handle(fb *Fiber, q *ServerRequest) Value {
	path, ok := s.resolve(q.url)
	if !ok {
		return serverFailure(fb, q, routeMissingTag)
	}
	fi, err := os.Stat(path)
	switch {
	case err != nil && isNotFound(systemError(err, "stat", "stat", path)):
		if accept, _ := q.header("accept"); s.spa && s.hasIndex && nodeExtname(path) == "" && strings.Contains(strings.ToLower(accept), "text/html") {
			return s.serve(fb, q, filepath.Join(s.root, s.index), -1)
		}
		return serverFailure(fb, q, routeMissingTag)
	case err != nil:
		return serverFailure(fb, q, internalErrorTag)
	case fi.Mode().IsRegular():
		return s.serve(fb, q, path, fi.Size())
	case fi.IsDir() && s.hasIndex:
		return s.serve(fb, q, filepath.Join(path, s.index), -1)
	}
	return serverFailure(fb, q, routeMissingTag)
}

// StaticServerMake answers the request with the files below the root; the
// go target calls it with the options and the request at once. The index is
// `index.html` unless the options name one.
func StaticServerMake(fb *Fiber, options, request Value) Value {
	o := record(options)
	root, _ := filepath.Abs(o.Get("root").(string))
	s := &staticServer{root: root, spa: o.Get("spa").(bool), mimeTypes: map[string]string{}}
	s.index, s.hasIndex = "index.html", true
	if i, ok := some(o.Get("index")); ok {
		s.index = i.(string)
	}
	if c, ok := some(o.Get("cacheControl")); ok {
		s.cacheControl, s.hasCache = c.(string), true
	}
	for _, p := range List(o.Get("mimeTypes")) {
		k, v := pairOf(p)
		s.mimeTypes[k] = v
	}
	return s.handle(fb, requestOf(request))
}
