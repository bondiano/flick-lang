package flickrt

import (
	"strconv"
	"strings"
)

// Codecs at the edge of the program: input checked to be inert data before
// decoding, and Standard Schema V1 validators for Go callers.

// inertProblem is where `in` meets a value again on the path that leads to
// it, as the JS runtime's check; a value shared by two paths is fine.
// Records and lists are the only values a Go caller can make cyclic.
func inertProblem(in Value) []issue {
	active := map[any]bool{}
	var walk func(v Value, path string) []issue
	enter := func(id any, path string, each func() []issue) []issue {
		if active[id] {
			return []issue{{path, "a cycle in the input"}}
		}
		active[id] = true
		defer delete(active, id)
		return each()
	}
	walk = func(v Value, path string) []issue {
		switch x := v.(type) {
		case *Record:
			return enter(x, path, func() []issue {
				for i, k := range x.keys {
					if p := walk(x.vals[i], atPath(path, k)); p != nil {
						return p
					}
				}
				return nil
			})
		case []Value:
			if len(x) == 0 {
				return nil
			}
			return enter(&x[0], path, func() []issue {
				for i, e := range x {
					if p := walk(e, path+"["+strconv.Itoa(i)+"]"); p != nil {
						return p
					}
				}
				return nil
			})
		}
		return nil
	}
	return walk(in, "")
}

// StandardSchema is `Codec.StandardSchema`: a validator Go code may call
// from any goroutine.
type StandardSchema struct {
	codec codec
	// made is the fiber of where an async validator was made, nil for a
	// synchronous one: its services, and the owner whose closing ends it
	made *Fiber
}

func CodecStandard(c Value) Value { return &StandardSchema{codec: codecOf(c)} }

func CodecStandardAsync(fb *Fiber, c Value) Value { return &StandardSchema{codecOf(c), fb} }

// Validate is `~standard.validate`: `{ value }`, or `{ issues }` each of a
// message and a path of keys.
func (s *StandardSchema) Validate(in Value) Value {
	fb := rootFiber()
	if s.made != nil {
		if o := s.made.owner; o != nil {
			o.mu.Lock()
			open := o.open
			o.mu.Unlock()
			if !open {
				return Rec("issues", []Value{Rec("message", "the scope of this validator has closed")})
			}
		}
		fb.env, fb.owner = s.made.env, s.made.owner
	}
	r, cause := catch(func() Value { return CodecDecodeResult(fb, s.codec, in) })
	if cause != nil {
		return Rec("issues", []Value{Rec("message", "the codec failed")})
	}
	if isOk(r) {
		return Rec("value", value(r))
	}
	var issues []Value
	for _, i := range List(value(r).(*Data).Fields()[0]) {
		path := []Value{}
		if p := record(i).Get("path").(string); p != "(root)" {
			for _, k := range strings.Split(p, ".") {
				path = append(path, k)
			}
		}
		issues = append(issues, Rec("message", record(i).Get("message"), "path", path))
	}
	return Rec("issues", issues)
}
