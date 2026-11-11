package flickrt

// `ConfigProvider`: where a Config reads its values. A provider gives the
// node at a path: a value, a record with its keys or an array with its
// length, where a record or an array may have a value too.

import (
	"os"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

const configProviderKey = "effect/ConfigProvider"

type cfgKind int

const (
	cfgValue cfgKind = iota
	cfgRecord
	cfgArray
)

type cfgNode struct {
	kind   cfgKind
	value  *string
	keys   []string
	length int
}

func valueNode(v string) *cfgNode { return &cfgNode{kind: cfgValue, value: &v} }

// A provider loads the node at a path, nil for none; a failure of its
// source is the message of its SourceError.
type provider interface {
	load(fb *Fiber, path []Value) (*cfgNode, *string)
	mapInput(f func(*Fiber, []Value) []Value) provider
}

type source struct {
	get       func(fb *Fiber, path []Value) (*cfgNode, *string)
	transform func(*Fiber, []Value) []Value
}

func (s *source) load(fb *Fiber, path []Value) (*cfgNode, *string) {
	if s.transform != nil {
		path = s.transform(fb, path)
	}
	return s.get(fb, path)
}

func (s *source) mapInput(f func(*Fiber, []Value) []Value) provider {
	inner := s.transform
	return &source{s.get, func(fb *Fiber, path []Value) []Value {
		if inner != nil {
			path = inner(fb, path)
		}
		return f(fb, path)
	}}
}

type orElseProvider struct{ first, second provider }

func (o *orElseProvider) load(fb *Fiber, path []Value) (*cfgNode, *string) {
	if node, err := o.first.load(fb, path); node != nil || err != nil {
		return node, err
	}
	return o.second.load(fb, path)
}

func (o *orElseProvider) mapInput(f func(*Fiber, []Value) []Value) provider {
	return &orElseProvider{o.first.mapInput(f), o.second.mapInput(f)}
}

var (
	sourceErrorTag = Tag("ConfigProvider.SourceError.SourceError")
	valueNodeTag   = Tag("ConfigProvider.Node.ValueNode")
	recordNodeTag  = Tag("ConfigProvider.Node.RecordNode")
)

func optionString(o Value) *string {
	if d := o.(*Data); d.Tag == "Some" {
		s := d.Fields[0].(string)
		return &s
	}
	return nil
}

// ConfigProviderMake is `ConfigProvider.make`: `load` gives the node at a
// path, and may fail with a SourceError.
func ConfigProviderMake(load Value) Value {
	return &source{get: func(fb *Fiber, path []Value) (*cfgNode, *string) {
		out, cause := catch(func() Value { return Run(fb, Apply(fb, load, path)) })
		if cause != nil {
			if cause.kind == causeFail {
				message := "the provider failed"
				if d, ok := cause.err.(*Data); ok && d.Tag == sourceErrorTag {
					message = d.Fields[0].(string)
				}
				return nil, &message
			}
			panic(&Failure{cause})
		}
		found := out.(*Data)
		if found.Tag != "Some" {
			return nil, nil
		}
		n := found.Fields[0].(*Data)
		switch n.Tag {
		case valueNodeTag:
			return valueNode(n.Fields[0].(string)), nil
		case recordNodeTag:
			return &cfgNode{kind: cfgRecord, keys: stringsOf(n.Fields[0]), value: optionString(n.Fields[1])}, nil
		}
		return &cfgNode{kind: cfgArray, length: int(n.Fields[0].(int32)), value: optionString(n.Fields[1])}, nil
	}}
}

func ConfigProviderOrElse(that, self Value) Value {
	return &orElseProvider{self.(provider), that.(provider)}
}

func ConfigProviderMapInput(f, self Value) Value {
	return self.(provider).mapInput(func(fb *Fiber, path []Value) []Value { return List(Apply(fb, f, path)) })
}

func ConfigProviderConstantCase(self Value) Value {
	return self.(provider).mapInput(func(_ *Fiber, path []Value) []Value {
		out := make([]Value, len(path))
		for i, seg := range path {
			if s, ok := seg.(string); ok {
				out[i] = ConfigCase(s)
			} else {
				out[i] = seg
			}
		}
		return out
	})
}

func ConfigProviderNested(prefix, self Value) Value {
	return self.(provider).mapInput(func(_ *Fiber, path []Value) []Value {
		return append([]Value{prefix}, path...)
	})
}

func emptyAsMissing(v *string, preserve bool) *string {
	if v != nil && *v == "" && !preserve {
		return nil
	}
	return v
}

func preserveEmpty(options Value) bool { return record(options).Get("preserveEmptyStrings").(bool) }

// jsKeys orders keys as a JS object's: integer keys ascending, then the
// others as they came.
func jsKeys(keys []string) []string {
	var ints, rest []string
	for _, k := range keys {
		if isArrayIndex(k) {
			ints = append(ints, k)
		} else {
			rest = append(rest, k)
		}
	}
	sort.Slice(ints, func(i, j int) bool { a, _ := strconv.Atoi(ints[i]); b, _ := strconv.Atoi(ints[j]); return a < b })
	return append(ints, rest...)
}

func isArrayIndex(k string) bool {
	n, err := strconv.ParseUint(k, 10, 32)
	return err == nil && strconv.FormatUint(n, 10) == k && n < 1<<32-1
}

type envTrie struct {
	keys     []string
	children map[string]*envTrie
}

func segmentText(seg Value) string {
	if s, ok := seg.(string); ok {
		return s
	}
	return strconv.Itoa(int(seg.(int32)))
}

// fromEnvMap is a provider over environment variables: `A_B` is the path
// `A`, `B`.
func fromEnvMap(names []string, env map[string]string, preserve bool) provider {
	root := &envTrie{}
	for _, name := range names {
		node := root
		for _, seg := range strings.Split(name, "_") {
			if node.children == nil {
				node.children = map[string]*envTrie{}
			}
			next, ok := node.children[seg]
			if !ok {
				next = &envTrie{}
				node.children[seg] = next
				node.keys = append(node.keys, seg)
			}
			node = next
		}
	}
	return &source{get: func(_ *Fiber, path []Value) (*cfgNode, *string) {
		parts := make([]string, len(path))
		node := root
		for i, seg := range path {
			parts[i] = segmentText(seg)
			if node != nil {
				node = node.children[parts[i]]
			}
		}
		var leaf *string
		if v, ok := env[strings.Join(parts, "_")]; ok {
			leaf = emptyAsMissing(&v, preserve)
		}
		if node == nil || len(node.keys) == 0 {
			if leaf == nil {
				return nil, nil
			}
			return valueNode(*leaf), nil
		}
		keys := jsKeys(node.keys)
		length := -1
		for _, k := range keys {
			if !isArrayIndex(k) {
				return &cfgNode{kind: cfgRecord, keys: keys, value: leaf}, nil
			}
			n, _ := strconv.Atoi(k)
			length = max(length, n)
		}
		return &cfgNode{kind: cfgArray, length: length + 1, value: leaf}, nil
	}}
}

func ConfigProviderFromEnvRecord(options, pairs Value) Value {
	env := map[string]string{}
	var names []string
	for _, p := range List(pairs) {
		kv := List(p)
		k := kv[0].(string)
		if _, seen := env[k]; !seen {
			names = append(names, k)
		}
		env[k] = kv[1].(string)
	}
	return fromEnvMap(names, env, preserveEmpty(options))
}

func ConfigProviderFromEnv(options Value) Value {
	env := map[string]string{}
	var names []string
	for _, kv := range os.Environ() {
		if k, v, ok := strings.Cut(kv, "="); ok {
			names = append(names, k)
			env[k] = v
		}
	}
	return fromEnvMap(names, env, preserveEmpty(options))
}

// ConfigProviderFromUnknown is `ConfigProvider.from_unknown` of a JSON value.
func ConfigProviderFromUnknown(options, root Value) Value {
	preserve := preserveEmpty(options)
	return &source{get: func(_ *Fiber, path []Value) (*cfgNode, *string) {
		cur := root
		for _, seg := range path {
			switch x := cur.(type) {
			case []Value:
				i, ok := seg.(int32)
				if !ok || i < 0 || int(i) >= len(x) {
					return nil, nil
				}
				cur = x[i]
			case *Record:
				k, ok := seg.(string)
				if !ok || x.index(k) < 0 {
					return nil, nil
				}
				cur = x.Get(k)
			default:
				return nil, nil
			}
		}
		switch x := cur.(type) {
		case nil:
			return nil, nil
		case string:
			if v := emptyAsMissing(&x, preserve); v != nil {
				return valueNode(*v), nil
			}
			return nil, nil
		case []Value:
			return &cfgNode{kind: cfgArray, length: len(x)}, nil
		case *Record:
			return &cfgNode{kind: cfgRecord, keys: jsKeys(x.keys)}, nil
		}
		if cur == JsonNull || cur == Unit {
			return nil, nil
		}
		return valueNode(StringFromAny(cur)), nil
	}}
}

// StringFromAny is JS `String` of a scalar.
func StringFromAny(v Value) string {
	switch x := v.(type) {
	case string:
		return x
	case bool:
		return strconv.FormatBool(x)
	case int32, float64:
		return StringFromNumber(x).(string)
	}
	return Format(v)
}

var dotEnvLine = regexp.MustCompile("(?m)^[ \\t\\f\\v]*(?:export\\s+)?([\\w.-]+)(?:\\s*=\\s*?|:\\s+?)(\\s*'(?:\\\\'|[^'])*'|\\s*\"(?:\\\\\"|[^\"])*\"|\\s*`(?:\\\\`|[^`])*`|[^#\\r\\n]+)?\\s*(?:#.*)?$")

// parseDotEnv is the variables of `.env` text, in order.
func parseDotEnv(text string) ([]string, map[string]string) {
	text = strings.ReplaceAll(strings.ReplaceAll(text, "\r\n", "\n"), "\r", "\n")
	env := map[string]string{}
	var names []string
	for _, m := range dotEnvLine.FindAllStringSubmatch(text, -1) {
		key, value := m[1], strings.TrimSpace(m[2])
		quote := byte(0)
		if value != "" {
			quote = value[0]
		}
		if len(value) >= 2 && strings.ContainsRune("'\"`", rune(quote)) && value[len(value)-1] == quote {
			value = value[1 : len(value)-1]
		}
		if quote == '"' {
			value = strings.ReplaceAll(strings.ReplaceAll(value, `\n`, "\n"), `\r`, "\r")
		}
		if _, seen := env[key]; !seen {
			names = append(names, key)
		}
		env[key] = value
	}
	return names, env
}

func isWordByte(c byte) bool {
	return c == '_' || c >= '0' && c <= '9' || c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z'
}

// dotEnvInterpolate expands `$NAME`, `${NAME}` and `${NAME:-default}`.
func dotEnvInterpolate(value string, env map[string]string, seen map[string]bool) string {
	var out strings.Builder
	cursor := 0
	for i := 0; i < len(value); i++ {
		if value[i] != '$' || i > 0 && value[i-1] == '\\' {
			continue
		}
		start := i + 1
		if start < len(value) && value[start] == '{' {
			start++
		}
		end := start
		for end < len(value) && isWordByte(value[end]) {
			end++
		}
		if end == start {
			continue
		}
		name, fallback := value[start:end], ""
		if strings.HasPrefix(value[end:], ":-") {
			end += 2
			from, depth := end, 0
			for ; end < len(value); end++ {
				if strings.HasPrefix(value[end:], "${") {
					depth++
					end++
				} else if value[end] == '}' {
					if depth == 0 {
						break
					}
					depth--
				}
			}
			fallback = value[from:end]
		}
		if end < len(value) && value[end] == '}' {
			end++
		}
		expanded := ""
		if v, ok := env[name]; ok && !seen[name] {
			inner := map[string]bool{name: true}
			for k := range seen {
				inner[k] = true
			}
			expanded = dotEnvInterpolate(v, env, inner)
		}
		if expanded == "" {
			expanded = dotEnvInterpolate(fallback, env, seen)
		}
		out.WriteString(value[cursor:i] + expanded)
		cursor, i = end, end-1
	}
	return out.String() + value[cursor:]
}

func ConfigProviderFromDotEnvContents(options, text Value) Value {
	o := record(options)
	names, env := parseDotEnv(text.(string))
	if o.Get("expandVariables").(bool) {
		expanded := make(map[string]string, len(env))
		for _, k := range names {
			expanded[k] = strings.ReplaceAll(dotEnvInterpolate(env[k], env, map[string]bool{k: true}), `\$`, "$")
		}
		env = expanded
	}
	return fromEnvMap(names, env, o.Get("preserveEmptyStrings").(bool))
}

// ConfigProviderFromDir is `ConfigProvider.from_dir`, read through the
// FileSystem service of the fiber that makes it.
func ConfigProviderFromDir(fb *Fiber, options Value) Value {
	o := record(options)
	root := "/"
	if r := optionString(o.Get("rootPath")); r != nil {
		root = *r
	}
	preserve := o.Get("preserveEmptyStrings").(bool)
	fs, ok := fb.env.get("FileSystem.FileSystem")
	if !ok {
		panic(Defect{"Service not found: FileSystem.FileSystem"})
	}
	impl := fs.(*Record)
	return &source{get: func(fb *Fiber, path []Value) (*cfgNode, *string) {
		parts := append([]string{root}, make([]string, len(path))...)
		for i, seg := range path {
			parts[i+1] = segmentText(seg)
		}
		full := pathJoin(parts)
		text, fileCause := catch(func() Value { return Run(fb, Apply(fb, impl.Get("read_file_string"), full)) })
		if fileCause == nil {
			trimmed := strings.TrimSpace(text.(string))
			if v := emptyAsMissing(&trimmed, preserve); v != nil {
				return valueNode(*v), nil
			}
			return nil, nil
		}
		entries, dirCause := catch(func() Value {
			return Run(fb, Apply(fb, Apply(fb, impl.Get("read_directory"), false), full))
		})
		if dirCause == nil {
			keys := stringsOf(entries)
			for i, k := range keys {
				keys[i] = pathBasename(k, "")
			}
			return &cfgNode{kind: cfgRecord, keys: keys}, nil
		}
		if notFound(fileCause) && notFound(dirCause) {
			return nil, nil
		}
		message := "Failed to read file at " + full
		return nil, &message
	}}
}

func notFound(c *Cause) bool {
	if c.kind != causeFail {
		panic(&Failure{c})
	}
	e, ok := c.err.(*Data)
	return ok && e.Tag == platformErrorTag && isNotFound(e)
}

func currentProvider(fb *Fiber) provider {
	if p, ok := fb.env.get(configProviderKey); ok {
		return p.(provider)
	}
	return ConfigProviderFromEnv(Rec("preserveEmptyStrings", false)).(provider)
}

// ConfigProviderLayer makes the provider the one of the body.
func ConfigProviderLayer(p Value) Value { return LayerSucceed(configProviderKey, p) }

// ConfigProviderLayerAdd adds the provider to the current one.
func ConfigProviderLayerAdd(options, p Value) Value {
	primary := record(options).Get("asPrimary").(bool)
	return fromBuild(func(fb *Fiber, _ *MemoMap, _ *Owner) Services {
		current := currentProvider(fb)
		if primary {
			return Services{configProviderKey: &orElseProvider{p.(provider), current}}
		}
		return Services{configProviderKey: &orElseProvider{current, p.(provider)}}
	})
}
