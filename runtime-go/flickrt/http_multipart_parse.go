package flickrt

import (
	"bytes"
	"errors"
	"io"
	"mime/multipart"
	"strings"
)

// The parser of `multipart/form-data` bodies, as Effect's MultipartParser:
// Go's mime/multipart splits the parts, and the limits, the media types
// and dispositions of parts, and which parts are files are Effect's.

// contentType is Effect's parse of a `Content-Type` (or, without a slash,
// `Content-Disposition`) header: the lower-cased value, "" when the header
// is malformed, and the parameters, later ones winning.
type contentType struct {
	value  string
	params map[string]string
}

func isTokenByte(c byte) bool {
	return c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || strings.IndexByte("!#$%&'*+.^_`|~-", c) >= 0
}

func isToken(s string) bool {
	for i := 0; i < len(s); i++ {
		if !isTokenByte(s[i]) {
			return false
		}
	}
	return s != ""
}

func parseContentType(header string, ok, withoutSlash bool) contentType {
	if !ok {
		return contentType{}
	}
	typ, rest, hasParams := strings.Cut(header, ";")
	typ = strings.TrimSpace(typ)
	main, sub, slash := strings.Cut(typ, "/")
	// a media type's tokens, unlike a parameter's, have no backtick
	if strings.Contains(typ, "`") || withoutSlash && (slash || !isToken(typ)) || !withoutSlash && (!slash || !isToken(main) || !isToken(sub)) {
		return contentType{}
	}
	out := contentType{value: strings.ToLower(typ), params: map[string]string{}}
	if !hasParams {
		return out
	}
	// each parameter is `; *token=(token|quoted) *`, back to back to the end
	s := ";" + rest
	for s != "" {
		if s[0] != ';' {
			return contentType{}
		}
		s = strings.TrimLeft(s[1:], " ")
		i := 0
		for i < len(s) && isTokenByte(s[i]) {
			i++
		}
		if i == 0 || i == len(s) || s[i] != '=' {
			return contentType{}
		}
		key, v := strings.ToLower(s[:i]), s[i+1:]
		var value string
		if strings.HasPrefix(v, `"`) {
			end, escaped := -1, false
			for j := 1; j < len(v); j++ {
				c := v[j]
				switch {
				case escaped:
					escaped = false
				case c == '\\':
					escaped = true
				case c == '"':
					end = j
				case c < 0x20 && c != 0x0b || c == 0x7f:
					return contentType{}
				}
				if end >= 0 {
					break
				}
			}
			if end < 0 {
				return contentType{}
			}
			value, v = v[1:end], v[end+1:]
			if !withoutSlash && strings.Contains(value, `\`) {
				var b strings.Builder
				for j := 0; j < len(value); j++ {
					if value[j] == '\\' && j+1 < len(value) {
						j++
					}
					b.WriteByte(value[j])
				}
				value = b.String()
			}
		} else {
			j := 0
			for j < len(v) && isTokenByte(v[j]) {
				j++
			}
			if j == 0 {
				return contentType{}
			}
			value, v = v[:j], v[j:]
		}
		out.params[key] = value
		s = strings.TrimLeft(v, " ")
	}
	return out
}

// partInfo is what the parser tells of a part.
type partInfo struct {
	name, contentType, charset string
	filename                   string
	hasFilename                bool
}

func defaultIsFile(info partInfo) bool {
	return info.hasFilename || info.contentType == "application/octet-stream"
}

// formLimits are the limits of a form, -1 for none, and the media types
// of parts read as fields.
type formLimits struct {
	maxParts, maxFieldSize, maxPartSize, maxTotalSize int64
	fieldTypes                                        []string
}

func (l *formLimits) isFile(info partInfo) bool {
	for _, t := range l.fieldTypes {
		if strings.Contains(info.contentType, t) {
			return false
		}
	}
	return defaultIsFile(info)
}

// The reasons a form is refused, as Effect's MultipartErrorReason names.
const (
	reasonTooManyParts  = "TooManyParts"
	reasonFieldTooLarge = "FieldTooLarge"
	reasonFileTooLarge  = "FileTooLarge"
	reasonBodyTooLarge  = "BodyTooLarge"
	reasonInternal      = "InternalError"
	reasonParse         = "Parse"
)

type formSink interface {
	field(info partInfo, value []byte)
	// file is where a file's bytes go, nil to drop them
	file(info partInfo) (io.WriteCloser, bool)
}

// countingReader fails once more than `max` bytes went through it, and
// sees the closing delimiter go by: mime/multipart ends a form cut short
// after a delimiter as it ends a whole one.
type countingReader struct {
	r        io.Reader
	n, max   int64
	exceeded bool
	closing  []byte // `\r\n--boundary--`
	tail     []byte // the stream's last bytes, after a virtual `\r\n`
	closed   bool
}

var errBodyTooLarge = errors.New("body too large")

func (c *countingReader) Read(p []byte) (int, error) {
	if c.exceeded {
		return 0, errBodyTooLarge
	}
	n, err := c.r.Read(p)
	if c.n += int64(n); c.max >= 0 && c.n > c.max {
		c.exceeded = true
		return 0, errBodyTooLarge
	}
	if !c.closed {
		c.tail = append(c.tail, p[:n]...)
		c.closed = bytes.Contains(c.tail, c.closing)
		if keep := len(c.closing) - 1; len(c.tail) > keep {
			c.tail = append(c.tail[:0], c.tail[len(c.tail)-keep:]...)
		}
	}
	return n, err
}

// headerSize is the size of a part's headers as sent: what Effect counts
// toward a part's size besides its body.
//
// shortcut: textproto keeps neither the headers' spelling nor their spacing, so the size is that of `Name: value` lines; count the raw bytes if a limit at the edge must match exactly
func headerSize(p *multipart.Part) int64 {
	n := int64(2 + 2) // the end of the boundary line, and the blank line
	for k, vs := range p.Header {
		for _, v := range vs {
			n += int64(len(k) + 2 + len(v) + 2)
		}
	}
	return n
}

// single is the header's value when the part has it once, as Effect reads
// a repeated one as malformed.
func single(p *multipart.Part, name string) (string, bool) {
	vs := p.Header[name]
	if len(vs) != 1 {
		return "", false
	}
	return decodeUTF8([]byte(vs[0])), true
}

// infoOfPart is Effect's info of a part, or false for a form-data part
// without a name.
func infoOfPart(p *multipart.Part) (partInfo, bool) {
	h, ok := single(p, "Content-Type")
	ct := parseContentType(h, ok, false)
	h, ok = single(p, "Content-Disposition")
	cd := parseContentType(h, ok, true)
	name, hasName := cd.params["name"]
	if cd.value == "form-data" && !hasName {
		return partInfo{}, false
	}
	info := partInfo{name: name, contentType: ct.value, charset: ct.params["charset"]}
	info.filename, info.hasFilename = cd.params["filename"]
	if encoded, ok := cd.params["filename*"]; ok {
		if parts := strings.Split(encoded, "''"); len(parts) == 2 {
			info.filename, info.hasFilename = parts[1], true
			if d, ok := decodeURIComponent(parts[1]); ok {
				info.filename = d
			}
		}
	}
	if info.contentType == "" {
		info.contentType = "text/plain"
		if _, ok := cd.params["filename"]; ok {
			info.contentType = "application/octet-stream"
		}
	}
	return info, true
}

// parseForm reads the parts of the body of the content type into the
// sink; the reason it refused the form, or "".
func parseForm(body io.Reader, contentTypeHeader string, hasType bool, limits *formLimits, sink formSink) string {
	boundary, ok := parseContentType(contentTypeHeader, hasType, false).params["boundary"]
	if !ok {
		return reasonParse
	}
	counted := &countingReader{r: body, max: limits.maxTotalSize, closing: []byte("\r\n--" + boundary + "--"), tail: []byte("\r\n")}
	reader := multipart.NewReader(counted, boundary)
	buf := make([]byte, 32*1024)
	var out io.WriteCloser
	defer func() {
		if out != nil {
			out.Close()
		}
	}()
	for parts := int64(1); ; parts++ {
		p, err := reader.NextRawPart()
		switch {
		case counted.exceeded:
			return reasonBodyTooLarge
		case err == io.EOF && counted.closed:
			return ""
		case err == io.EOF:
			return reasonParse
		case err != nil:
			return reasonParse
		case limits.maxParts >= 0 && parts > limits.maxParts:
			return reasonTooManyParts
		}
		info, ok := infoOfPart(p)
		if !ok {
			return reasonParse
		}
		size := headerSize(p)
		if limits.maxPartSize >= 0 && size > limits.maxPartSize {
			return reasonFileTooLarge
		}
		isFile := limits.isFile(info)
		var field []byte
		if isFile {
			if out, ok = sink.file(info); !ok {
				return reasonInternal
			}
		}
		for {
			n, err := p.Read(buf)
			if size += int64(n); limits.maxPartSize >= 0 && size > limits.maxPartSize {
				return reasonFileTooLarge
			}
			if isFile {
				if out != nil && n > 0 {
					if _, werr := out.Write(buf[:n]); werr != nil {
						return reasonInternal
					}
				}
			} else if field = append(field, buf[:n]...); limits.maxFieldSize >= 0 && int64(len(field)) > limits.maxFieldSize {
				return reasonFieldTooLarge
			}
			if err == io.EOF {
				break
			}
			if counted.exceeded {
				return reasonBodyTooLarge
			}
			if err != nil {
				return reasonParse
			}
		}
		if !isFile {
			sink.field(info, field)
		} else if out != nil {
			err, out = out.Close(), nil
			if err != nil {
				return reasonInternal
			}
		}
	}
}
