package flickrt

import (
	"sort"
	"strings"
	"sync"
)

// Http.Mime: media types by extension and back, over the table of the
// `mime` package.

type mimeTables struct {
	extensionToType map[string]string
	typeToExtension map[string]string
	typeToAll       map[string][]string
}

var mimeOnce = sync.OnceValue(func() *mimeTables {
	t := &mimeTables{map[string]string{}, map[string]string{}, map[string][]string{}}
	for _, line := range strings.Split(mimeTypes, "\n") {
		fields := strings.Fields(line)
		typ := fields[0]
		var all []string
		for i, ext := range fields[1:] {
			starred := strings.HasPrefix(ext, "*")
			ext = strings.TrimPrefix(ext, "*")
			all = append(all, ext)
			if i == 0 {
				t.typeToExtension[typ] = ext
			}
			if !starred {
				t.extensionToType[ext] = typ
			}
		}
		t.typeToAll[typ] = all
	}
	return t
})

// MimeGetType is the type of the path's extension; a path with a
// directory needs a dot in its last part.
func MimeGetType(path Value) Value {
	p := path.(string)
	last := ToLowerCase(p[strings.LastIndexAny(p, "/\\")+1:]).(string)
	ext := last[strings.LastIndexByte(last, '.')+1:]
	hasPath := Length(last).(int32) < Length(p).(int32)
	hasDot := Length(ext).(int32) < Length(last).(int32)-1
	if !hasDot && hasPath {
		return None
	}
	if t, ok := mimeOnce().extensionToType[ext]; ok {
		return Some(t)
	}
	return None
}

func mimeKey(typ Value) string {
	t, _, _ := strings.Cut(typ.(string), ";")
	return ToLowerCase(Trim(t)).(string)
}

func MimeGetExtension(typ Value) Value {
	if e, ok := mimeOnce().typeToExtension[mimeKey(typ)]; ok {
		return Some(e)
	}
	return None
}

// MimeGetAllExtensions is every extension of the type, sorted as JS sorts
// strings.
func MimeGetAllExtensions(typ Value) Value {
	all, ok := mimeOnce().typeToAll[mimeKey(typ)]
	if !ok {
		return None
	}
	seen := map[string]bool{}
	out := []Value{}
	for _, e := range all {
		if !seen[e] {
			seen[e] = true
			out = append(out, e)
		}
	}
	sort.Slice(out, func(i, j int) bool { return CompareStrings(out[i].(string), out[j].(string)) < 0 })
	return Some(out)
}
