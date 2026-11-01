package flickrt

import (
	"io"
	"strings"
	"testing"
)

type fieldSink struct{ fields []string }

func (s *fieldSink) field(info partInfo, value []byte) {
	s.fields = append(s.fields, info.name+"="+string(value))
}
func (s *fieldSink) file(partInfo) (io.WriteCloser, bool) { return nil, true }

func TestParseFormLimits(t *testing.T) {
	body := "--b\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n1\r\n" +
		"--b\r\nContent-Disposition: form-data; name=\"b\"\r\n\r\n22\r\n--b--\r\n"
	limits := func(f func(*formLimits)) *formLimits {
		l := &formLimits{maxParts: -1, maxFieldSize: -1, maxPartSize: -1, maxTotalSize: -1}
		f(l)
		return l
	}
	for _, c := range []struct {
		name, contentType, want string
		limits                  *formLimits
	}{
		{"accepted", "multipart/form-data; boundary=b", "", limits(func(*formLimits) {})},
		{"parts", "multipart/form-data; boundary=b", reasonTooManyParts, limits(func(l *formLimits) { l.maxParts = 1 })},
		{"field", "multipart/form-data; boundary=b", reasonFieldTooLarge, limits(func(l *formLimits) { l.maxFieldSize = 1 })},
		{"part", "multipart/form-data; boundary=b", reasonFileTooLarge, limits(func(l *formLimits) { l.maxPartSize = 10 })},
		{"total", "multipart/form-data; boundary=b", reasonBodyTooLarge, limits(func(l *formLimits) { l.maxTotalSize = 10 })},
		{"no boundary", "multipart/form-data", reasonParse, limits(func(*formLimits) {})},
	} {
		sink := &fieldSink{}
		if got := parseForm(strings.NewReader(body), c.contentType, true, c.limits, sink); got != c.want {
			t.Errorf("%s: %q, want %q", c.name, got, c.want)
		}
		if c.want == "" && strings.Join(sink.fields, ",") != "a=1,b=22" {
			t.Errorf("%s: fields %v", c.name, sink.fields)
		}
	}
	if got := parseForm(strings.NewReader("--b\r\n"), "multipart/form-data; boundary=b", true, limits(func(*formLimits) {}), &fieldSink{}); got != reasonParse {
		t.Errorf("unended: %q", got)
	}
}

func TestParseContentType(t *testing.T) {
	ct := parseContentType(`Multipart/Form-Data; Boundary="a\"b"; x=1 `, true, false)
	if ct.value != "multipart/form-data" || ct.params["boundary"] != `a"b` || ct.params["x"] != "1" {
		t.Errorf("parsed %+v", ct)
	}
	for _, bad := range []string{"text", "text/plain;", "text/plain; a", `text/plain; a="b`, "te`xt/plain"} {
		if parseContentType(bad, true, false).value != "" {
			t.Errorf("%q parsed", bad)
		}
	}
	if cd := parseContentType(`form-data; name="f"; filename="a.txt"`, true, true); cd.value != "form-data" || cd.params["filename"] != "a.txt" {
		t.Errorf("disposition %+v", cd)
	}
	if persistedFileName("../x/y.txt") != "y.txt" || persistedFileName("..") != "file" || len(persistedFileName(strings.Repeat("é", 100))) != 128 {
		t.Error("persisted file names")
	}
}

// A response's form data keeps the text fields, as a web Response does.
func TestMultipartTexts(t *testing.T) {
	body := "--XX\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\n1\r\n" +
		"--XX\r\nContent-Disposition: form-data; name=\"f\"; filename=\"x.txt\"\r\n\r\nhi\r\n" +
		"--XX\r\nContent-Disposition: form-data; name=\"b\"\r\n\r\n2\r\n--XX--\r\n"
	fields, ok := multipartTexts([]byte(body), "XX")
	if got := Format(fields); !ok || got != `[["a","1"],["b","2"]]` {
		t.Errorf("fields %v %s", ok, got)
	}
	if _, ok := multipartTexts([]byte("garbage"), "XX"); ok {
		t.Error("garbage parsed")
	}
}
