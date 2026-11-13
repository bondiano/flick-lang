package flickrt

import "strings"

// The Flick service `Path.Path` of Effect's `Path.layer` and of Node's
// (`Path.node`), and their file URLs: Effect's own for the first, Node's
// `url` module for the second, which differ in what their errors say.

func pathBadArgument(description Value) Value {
	return NewData(platformErrorTag, NewData(Tag("PlatformError.Reason.Bad"),
		Rec("module_", "Path", "method", "fromFileUrl", "description", description)))
}

// fromFileURL is the path of a `file:` URL; `node` fails as Node's
// `fileURLToPath` does, without a description.
func fromFileURL(text string, node bool) Value {
	fail := func(description string) Value {
		if node {
			return Fail(pathBadArgument(None))
		}
		return Fail(pathBadArgument(Some(description)))
	}
	u, ok := parseURL(text)
	if !ok {
		return Fail(pathBadArgument(Some("invalid URL " + text)))
	}
	if u.scheme != "file" {
		return fail("URL must be of scheme file")
	}
	if u.host != "" {
		return fail("Invalid file URL host")
	}
	lower := strings.ToLower(u.path)
	if strings.Contains(lower, "%2f") {
		return fail("must not include encoded / characters")
	}
	path, ok := decodeURIComponent(u.path)
	if !ok {
		panic(Defect{"URIError: URI malformed"})
	}
	return path
}

// toFileURL is the `file:` URL of the path resolved, a trailing slash kept.
func toFileURL(path string) string {
	resolved := pathResolve(path)
	if strings.HasSuffix(path, "/") && !strings.HasSuffix(resolved, "/") {
		resolved += "/"
	}
	resolved = strings.NewReplacer("%", "%25", "\\", "%5C", "\n", "%0A", "\r", "%0D", "\t", "%09").Replace(resolved)
	u := webURL{scheme: "file", hasHost: true}
	u.setPath(resolved)
	return u.href()
}

func pathService(node bool) Value {
	sync := func(f func(string) Value) Value {
		return Fn(func(p Value) Value { return Eff(func(*Fiber) Value { return f(p.(string)) }) })
	}
	return Rec(
		"sep", Fn(func(Value) Value { return Succeed("/") }),
		"basename", sync(func(p string) Value { return pathBasename(p, "") }),
		"basename_without", Curry2(func(suffix, p Value) Value {
			return Succeed(pathBasename(p.(string), suffix.(string)))
		}),
		"dirname", sync(func(p string) Value { return pathDirname(p) }),
		"extname", sync(func(p string) Value { return pathExtname(p) }),
		"format", Fn(func(parsed Value) Value { return Eff(func(*Fiber) Value { return pathFormat(parsed) }) }),
		"from_file_url", sync(func(url string) Value { return fromFileURL(url, node) }),
		"is_absolute", sync(func(p string) Value { return strings.HasPrefix(p, "/") }),
		"join", Fn(func(paths Value) Value { return Eff(func(*Fiber) Value { return pathJoin(List(paths)) }) }),
		"normalize", sync(func(p string) Value { return pathNormalize(p) }),
		"parse", sync(pathParse),
		"relative", Curry2(func(from, to Value) Value {
			return Eff(func(*Fiber) Value { return pathRelative(from.(string), to.(string)) })
		}),
		"resolve", Fn(func(paths Value) Value {
			return Eff(func(*Fiber) Value {
				ps := make([]string, 0, len(List(paths)))
				for _, p := range List(paths) {
					ps = append(ps, p.(string))
				}
				return pathResolve(ps...)
			})
		}),
		"resolve_within", Curry2(func(root, input Value) Value {
			return Eff(func(*Fiber) Value { return pathResolveWithin(root.(string), input.(string)) })
		}),
		"to_file_url", sync(func(p string) Value { return toFileURL(p) }),
		"to_namespaced_path", sync(func(p string) Value { return p }),
	)
}

func PathLayer(key Value) Value { return LayerSucceed(key, pathService(false)) }
func PathNode(key Value) Value  { return LayerSucceed(key, pathService(true)) }
