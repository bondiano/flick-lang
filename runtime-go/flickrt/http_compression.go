package flickrt

import (
	"bytes"
	"compress/gzip"
	"compress/zlib"
	"io"
	"maps"
	"math/big"
	"regexp"
	"strconv"
	"strings"
	"sync"
)

// The compression middlewares as Effect's: responses compressed by the
// request's `Accept-Encoding` before they are sent, request bodies
// decompressed by their `Content-Encoding`.

var (
	acceptMember = regexp.MustCompile(`^([a-z0-9!#$%&'*+.^_` + "`" + `|~-]+)(?:;q=(0(?:\.[0-9]{0,3})?|1(?:\.0{0,3})?))?$`)
	paramSpaces  = regexp.MustCompile(`[ \t]*;[ \t]*`)
	noTransform  = regexp.MustCompile(`(?i)(?:^|[\s,])no-transform(?:$|[\s,;])`)
	algorithmOf  = map[string]string{"Gzip": "gzip", "Deflate": "deflate", "Brotli": "br", "Zstd": "zstd"}
)

// negotiate is the first preferred algorithm of `supported` the header
// accepts, or "".
func negotiate(header string, preferred []string, supported map[string]openCompressor) string {
	header = strings.TrimSpace(header)
	if header == "" {
		return ""
	}
	accepted := map[string]float64{}
	for _, part := range strings.Split(header, ",") {
		member := paramSpaces.ReplaceAllString(strings.ToLower(strings.TrimSpace(part)), ";")
		m := acceptMember.FindStringSubmatch(member)
		if m == nil {
			return ""
		}
		q := 1.0
		if m[2] != "" {
			q, _ = strconv.ParseFloat(m[2], 64)
		}
		accepted[m[1]] = q
	}
	for _, a := range preferred {
		if supported[a] == nil {
			continue
		}
		q, ok := accepted[a]
		if !ok {
			q, ok = accepted["*"]
		}
		if ok && q > 0 {
			return a
		}
	}
	return ""
}

func compressible(contentType string) bool {
	t, _, _ := strings.Cut(contentType, ";")
	t = strings.ToLower(strings.TrimSpace(t))
	switch t {
	case "application/json", "application/javascript", "application/xml", "image/svg+xml", "application/wasm":
		return true
	}
	return strings.HasPrefix(t, "text/") || strings.HasSuffix(t, "+json") || strings.HasSuffix(t, "+xml")
}

// Compressor writes what is written to it compressed.
type Compressor interface {
	io.WriteCloser
	Flush() error
}

type openCompressor = func(level int, w io.Writer) (Compressor, error)

// webCompressors are the algorithms of the web's CompressionStream, and
// compressors also those an optional package registers.
var (
	webCompressors = map[string]openCompressor{
		"gzip":    func(level int, w io.Writer) (Compressor, error) { return gzip.NewWriterLevel(w, level) },
		"deflate": func(level int, w io.Writer) (Compressor, error) { return zlib.NewWriterLevel(w, level) },
	}
	compressors = maps.Clone(webCompressors)
)

// RegisterCompression adds an algorithm to those `compression` answers
// with: flickcompress registers br and zstd from its init, so a program
// that never compresses carries neither module.
func RegisterCompression(algorithm string, open func(level int, w io.Writer) (Compressor, error)) {
	compressors[algorithm] = open
}

// compress compresses data in one write, which brotli also takes for its
// size hint, as Node's brotliCompress is given the size of the body.
func compress(algorithm string, level int, data []byte) ([]byte, error) {
	var buf bytes.Buffer
	w, err := compressors[algorithm](level, &buf)
	if err != nil {
		return nil, err
	}
	if _, err = w.Write(data); err == nil {
		err = w.Close()
	}
	return buf.Bytes(), err
}

// compressingReader compresses a body as it is read, each chunk read from
// it flushed, so a streamed response streams compressed too.
type compressingReader struct {
	in  io.ReadCloser
	w   Compressor
	out bytes.Buffer
	buf []byte
	eof bool
}

func newCompressingReader(open openCompressor, level int, in io.ReadCloser) (io.ReadCloser, error) {
	r := &compressingReader{in: in, buf: make([]byte, 32<<10)}
	w, err := open(level, &r.out)
	if err != nil {
		in.Close()
		return nil, err
	}
	r.w = w
	return r, nil
}

func (r *compressingReader) Read(p []byte) (int, error) {
	for r.out.Len() == 0 {
		if r.eof {
			return 0, io.EOF
		}
		n, err := r.in.Read(r.buf)
		if n > 0 {
			r.w.Write(r.buf[:n])
			r.w.Flush()
		}
		if err == io.EOF {
			r.eof = true
			r.w.Close()
		} else if err != nil {
			return 0, err
		}
	}
	return r.out.Read(p)
}

func (r *compressingReader) Close() error { return r.in.Close() }

func withVary(response Value) Value {
	return ResponseSetHeader("vary", varyWith(responseOf(response).headerRecord(), "Accept-Encoding"), response)
}

// MiddlewareCompression compresses the response before it is sent.
func MiddlewareCompression(fb *Fiber, options, handler, request Value) Value {
	return compression(fb, options, handler, request, false)
}

// MiddlewareCompressionWeb is the compression of the web's
// CompressionStream, whose level is zlib's default whatever the options say.
func MiddlewareCompressionWeb(fb *Fiber, options, handler, request Value) Value {
	return compression(fb, options, handler, request, true)
}

func compression(fb *Fiber, options, handler, request Value, web bool) Value {
	o := record(options)
	preferred := []string{"br", "gzip", "deflate"}
	if as := List(o.Get("algorithms")); len(as) > 0 {
		preferred = preferred[:0:0]
		for _, a := range as {
			preferred = append(preferred, algorithmOf[ConstructorName(a.(*Data).Tag)])
		}
	}
	minSize := int64(1024)
	if m, ok := some(o.Get("minSize")); ok {
		minSize = int64(m.(float64))
	}
	supported := compressors
	if web {
		supported = webCompressors
	}
	levels := map[string]int{"gzip": 6, "deflate": 6, "br": 4, "zstd": 3}
	for _, l := range List(o.Get("levels")) {
		if web {
			break
		}
		pair := l.([]Value)
		levels[algorithmOf[ConstructorName(pair[0].(*Data).Tag)]] = int(pair[1].(int32))
	}
	appendPreResponse(fb, func(fb *Fiber, request, response Value) Value {
		r := responseOf(response)
		if r.status < 200 || r.status == 204 || r.status == 206 || r.status == 304 {
			return response
		}
		headers := r.headerRecord()
		if enc, ok := recordField(headers, "content-encoding").(string); ok {
			if strings.ToLower(strings.TrimSpace(enc)) == "identity" {
				return ResponseRemoveHeader("content-encoding", response)
			}
			return response
		}
		if r.body.kind == bodyEmpty || r.body.kind == bodyForm {
			return response
		}
		if cc, ok := recordField(headers, "cache-control").(string); ok && noTransform.MatchString(cc) {
			return response
		}
		ct, ok := recordField(headers, "content-type").(string)
		if !ok && r.body.hasType {
			ct, ok = r.body.contentType, true
		}
		if !ok || !compressible(ct) {
			return response
		}
		accept, _ := requestOf(request).header("accept-encoding")
		algorithm := negotiate(accept, preferred, supported)
		if algorithm == "" {
			return withVary(response)
		}
		length := r.body.length
		if length < 0 {
			length = parseContentLength(recordField(headers, "content-length"))
		}
		if length >= 0 && length < minSize {
			return withVary(response)
		}
		var body *Body
		if r.body.kind == bodyStream {
			source, compressor, level := r.body, supported[algorithm], levels[algorithm]
			body = &Body{kind: bodyStream, hasType: true, contentType: ct, length: -1,
				open: func(fb *Fiber) (io.ReadCloser, error) {
					in, err := source.open(fb)
					if err != nil {
						return nil, err
					}
					return newCompressingReader(compressor, level, in)
				}}
		} else {
			data, _, err := r.body.bytes(fb)
			if err == nil {
				data, err = compress(algorithm, levels[algorithm], data)
			}
			if err != nil {
				panic(Defect{err.Error()})
			}
			body = &Body{kind: bodyBytes, hasType: true, contentType: ct, length: -1, data: string(data)}
		}
		compressed := ResponseRemoveHeader("content-length", ResponseSetBody(body, response))
		set := newRecord(3)
		set.put("content-encoding", algorithm)
		set.put("vary", varyWith(responseOf(compressed).headerRecord(), "Accept-Encoding"))
		if etag, ok := recordField(headers, "etag").(string); ok && !strings.HasPrefix(etag, "W/") {
			set.put("etag", "W/"+etag)
		}
		return ResponseSetHeaders(HeadersToList(set), compressed)
	})
	return handle(fb, handler, request)
}

// inflation is the decompression of a request's body: its codings, in the
// order they were applied, and the largest size it may grow to.
type inflation struct {
	codings []string
	limit   *big.Int // nil: the request's largest body size, or 10 MiB
	once    sync.Once
	body    []byte
	err     error
}

const defaultMaxDecompressedSize = 10 << 20

func (in *inflation) read(fb *Fiber, q *ServerRequest) ([]byte, error) {
	in.once.Do(func() {
		limit := int64(defaultMaxDecompressedSize)
		switch {
		case in.limit != nil:
			limit = in.limit.Int64()
		case fb != nil:
			if v, ok := fb.env.get(maxBodySizeKey); ok {
				limit = v.(*big.Int).Int64()
			}
		}
		data, err := q.src.read(fb)
		for i := len(in.codings) - 1; i >= 0 && err == nil; i-- {
			var r io.ReadCloser
			if in.codings[i] == "gzip" {
				r, err = gzip.NewReader(bytes.NewReader(data))
			} else {
				r, err = zlib.NewReader(bytes.NewReader(data))
			}
			if err == nil {
				data, err = io.ReadAll(io.LimitReader(r, limit+1))
				r.Close()
			}
			if err == nil && int64(len(data)) > limit {
				err = errString("Decompressed request body exceeds " + strconv.FormatInt(limit, 10) + " bytes")
			}
		}
		in.body, in.err = data, err
	})
	return in.body, in.err
}

// MiddlewareDecompression decompresses the request's body, refusing with
// 415 one of an encoding other than gzip and deflate.
func MiddlewareDecompression(fb *Fiber, max, handler, request Value) Value {
	q := requestOf(request)
	encoding, ok := q.header("content-encoding")
	if !ok || !hasBody(q.method) {
		return handle(fb, handler, request)
	}
	var codings []string
	for _, member := range strings.Split(encoding, ",") {
		coding := strings.ToLower(strings.TrimSpace(member))
		if coding == "" || coding == "identity" {
			continue
		}
		if coding != "gzip" && coding != "deflate" {
			h := headersOf(HeadersEmpty).set("accept-encoding", "gzip, deflate")
			return newResponse(415, "", false, h, nil, nil, false)
		}
		codings = append(codings, coding)
	}
	if len(codings) == 0 {
		return handle(fb, handler, request)
	}
	in := &inflation{codings: codings}
	if m, ok := some(max); ok {
		in.limit = m.(*big.Int)
	}
	q = q.modified()
	q.inflate = in
	q.headers = q.headerRecord().without("content-encoding").without("content-length")
	return handle(fb, handler, q)
}
