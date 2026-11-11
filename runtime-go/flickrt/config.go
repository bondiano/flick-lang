package flickrt

// `Config`: a description of configuration read from the current
// ConfigProvider (the environment by default) when it runs. Reading gives
// a value, or a recoverable failure where the provider has nothing (which
// `with_default` and `or_else` take); a value that does not parse and a
// failed source fail with a ConfigError at once.

import (
	"math"
	"math/big"
	"regexp"
	"strings"
)

var configErrorTag = Tag("Config.ConfigError.ConfigError")

func configError(message string) Value { return NewData(configErrorTag, message) }

// A config reads at the path prefix; `missing` is the ConfigError of a
// recoverable failure.
type config struct {
	eval func(fb *Fiber, p provider, prefix []Value) (v Value, missing Value)
}

func cfg(c Value) *config { return c.(*config) }

func (c *config) parse(fb *Fiber, p provider) Value {
	v, missing := c.eval(fb, p, nil)
	if missing != nil {
		return Fail(missing)
	}
	return v
}

func ConfigGet(fb *Fiber, c Value) Value { Check(fb); return cfg(c).parse(fb, currentProvider(fb)) }

func ConfigParse(fb *Fiber, p, c Value) Value { return cfg(c).parse(fb, p.(provider)) }

func ConfigMap(f, self Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		v, missing := cfg(self).eval(fb, p, prefix)
		if missing != nil {
			return nil, missing
		}
		return Apply(fb, f, v), nil
	}}
}

func ConfigFlatMap(f, self Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		v, missing := cfg(self).eval(fb, p, prefix)
		if missing != nil {
			return nil, missing
		}
		return cfg(Apply(fb, f, v)).eval(fb, p, prefix)
	}}
}

// ConfigMapEffect checks or converts the value; `f` failing is a failure
// of reading.
func ConfigMapEffect(f, self Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		v, missing := cfg(self).eval(fb, p, prefix)
		if missing != nil {
			return nil, missing
		}
		return Run(fb, Apply(fb, f, v)), nil
	}}
}

// ConfigOrElse reads what `f` picks by the error of a failure; defects and
// interruptions pass.
func ConfigOrElse(f, self Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		var out, missing Value
		_, cause := catch(func() Value { out, missing = cfg(self).eval(fb, p, prefix); return Unit })
		switch {
		case cause != nil && cause.kind == causeFail:
			missing = cause.err
		case cause != nil:
			panic(&Failure{cause})
		case missing == nil:
			return out, nil
		}
		return cfg(Apply(fb, f, missing)).eval(fb, p, prefix)
	}}
}

func ConfigAll(configs Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		cs := List(configs)
		out := make([]Value, len(cs))
		var first Value
		for i, c := range cs {
			v, missing := cfg(c).eval(fb, p, prefix)
			if missing != nil && first == nil {
				first = missing
			}
			out[i] = v
		}
		if first != nil {
			return nil, first
		}
		return out, nil
	}}
}

func ConfigWithDefault(value, self Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		v, missing := cfg(self).eval(fb, p, prefix)
		if missing != nil {
			return value, nil
		}
		return v, nil
	}}
}

func ConfigOption(self Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		v, missing := cfg(self).eval(fb, p, prefix)
		if missing != nil {
			return None, nil
		}
		return Some(v), nil
	}}
}

func ConfigFail(message Value) Value {
	return &config{func(*Fiber, provider, []Value) (Value, Value) {
		return Fail(configError("SourceError: " + message.(string))), nil
	}}
}

func ConfigSucceed(value Value) Value {
	return &config{func(*Fiber, provider, []Value) (Value, Value) { return value, nil }}
}

func ConfigNested(name, self Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		return cfg(self).eval(fb, p, append(append([]Value{}, prefix...), name))
	}}
}

// load is the node at the path; a failed source fails reading at once.
func load(fb *Fiber, p provider, path []Value) *cfgNode {
	node, sourceErr := p.load(fb, path)
	if sourceErr != nil {
		Fail(configError("SourceError: " + *sourceErr))
	}
	return node
}

// schemaError is the ConfigError of a SchemaError at the path.
func schemaError(path []Value, message string) Value {
	if len(path) > 0 {
		var at strings.Builder
		for _, seg := range path {
			if s, ok := seg.(string); ok {
				at.WriteString("[" + QuoteJSON(s) + "]")
			} else {
				at.WriteString("[" + segmentText(seg) + "]")
			}
		}
		message += "\n  at " + at.String()
	}
	return configError("SchemaError(" + message + ")")
}

// A decoder reads the text of a value, or gives the message of an issue.
type decoder func(text string) (Value, string)

// scalar reads the value at the path, missing where the provider has none.
func scalar(name Value, decode decoder) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		path := append(append([]Value{}, prefix...), name)
		node := load(fb, p, path)
		if node == nil || node.value == nil {
			return nil, schemaError(path, "Expected string")
		}
		v, issue := decode(*node.value)
		if issue != "" {
			return Fail(schemaError(path, issue)), nil
		}
		return v, nil
	}}
}

func decodeString(text string) (Value, string) { return text, "" }

func decodeNonEmpty(text string) (Value, string) {
	if text == "" {
		return nil, "Expected a value with a length of at least 1"
	}
	return text, ""
}

func decodeNumber(text string) (Value, string) {
	x, issues := numberOfText(text)
	if issues != nil {
		return nil, "Expected a string representing a finite number"
	}
	return x, ""
}

func decodeFinite(text string) (Value, string) {
	x, issue := decodeNumber(text)
	if issue != "" || math.IsNaN(x.(float64)) || math.IsInf(x.(float64), 0) {
		return nil, "Expected a string representing a finite number"
	}
	return x, ""
}

func configBetween(min, max float64) decoder {
	return func(text string) (Value, string) {
		v, issue := decodeFinite(text)
		if issue != "" {
			return nil, issue
		}
		x := v.(float64)
		if x != math.Trunc(x) {
			return nil, "Expected an integer"
		}
		if x < min || x > max {
			return nil, "Expected a value between " + StringFromNumber(min).(string) + " and " + StringFromNumber(max).(string)
		}
		return x, ""
	}
}

func decodeInt(text string) (Value, string) {
	v, issue := configBetween(math.MinInt32, math.MaxInt32)(text)
	if issue != "" {
		return nil, issue
	}
	return int32(v.(float64)), ""
}

func literals(options []string) decoder {
	quoted := make([]string, len(options))
	for i, o := range options {
		quoted[i] = QuoteJSON(o)
	}
	expected := "Expected " + strings.Join(quoted, " | ")
	return func(text string) (Value, string) {
		for _, o := range options {
			if text == o {
				return text, ""
			}
		}
		return nil, expected
	}
}

var trueTexts, booleanTexts = []string{"true", "yes", "on", "1", "y"}, literals([]string{"true", "yes", "on", "1", "y", "false", "no", "off", "0", "n"})

func decodeBoolean(text string) (Value, string) {
	if _, issue := booleanTexts(text); issue != "" {
		return nil, issue
	}
	for _, t := range trueTexts {
		if text == t {
			return true, ""
		}
	}
	return false, ""
}

func decodeDuration(text string) (Value, string) {
	r := DurationFromInput(text).(*Data)
	if r.Tag != "Success" {
		return nil, "Expected a valid Duration string"
	}
	return r.Fields()[0], ""
}

var byteSizeText = regexp.MustCompile(`^\s*(\d+)(?:\.(\d+))?\s*([A-Za-z]+)\s*$`)

var byteSizeUnits = func() map[string]*big.Int {
	units := map[string]*big.Int{}
	pow := func(base, n int64) *big.Int { return new(big.Int).Exp(big.NewInt(base), big.NewInt(n), nil) }
	add := func(f *big.Int, names ...string) {
		for _, n := range names {
			units[n] = f
		}
	}
	add(big.NewInt(1), "B", "byte", "bytes")
	for i, p := range []string{"k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"} {
		words := []string{"kilo", "mega", "giga", "tera", "peta", "exa", "zetta", "yotta", "ronna", "quetta"}[i]
		add(pow(1000, int64(i+1)), p+"B", words+"byte", words+"bytes")
		if i < 8 {
			binary := []string{"kibi", "mebi", "gibi", "tebi", "pebi", "exbi", "zebi", "yobi"}[i]
			add(pow(1024, int64(i+1)), strings.ToUpper(p)+"iB", binary+"byte", binary+"bytes")
		}
	}
	return units
}()

func decodeByteSize(text string) (Value, string) {
	m := byteSizeText.FindStringSubmatch(text)
	if m == nil || byteSizeUnits[m[3]] == nil {
		return nil, "Expected a valid ByteSize string"
	}
	n, _ := new(big.Int).SetString(m[1]+m[2], 10)
	n.Mul(n, byteSizeUnits[m[3]])
	scale := new(big.Int).Exp(big.NewInt(10), big.NewInt(int64(len(m[2]))), nil)
	q, r := new(big.Int).QuoRem(n, scale, new(big.Int))
	if r.Sign() != 0 {
		return nil, "Expected a valid ByteSize string"
	}
	return q, ""
}

func decodeURL(text string) (Value, string) {
	u, ok := parseURL(text)
	if !ok {
		return nil, "Expected a string that will be decoded as a URL"
	}
	return u.href(), ""
}

// decodeDate is JS `new Date(text)`, as milliseconds.
func decodeDate(text string) (Value, string) {
	if ms := parseJsDate(text); !math.IsNaN(ms) {
		return ms, ""
	}
	return nil, "Expected a valid Date"
}

func ConfigString(name Value) Value         { return scalar(name, decodeString) }
func ConfigNonEmptyString(name Value) Value { return scalar(name, decodeNonEmpty) }
func ConfigNumber(name Value) Value         { return scalar(name, decodeNumber) }
func ConfigFinite(name Value) Value         { return scalar(name, decodeFinite) }
func ConfigInt(name Value) Value            { return scalar(name, decodeInt) }
func ConfigBoolean(name Value) Value        { return scalar(name, decodeBoolean) }
func ConfigPort(name Value) Value           { return scalar(name, configBetween(1, 65535)) }
func ConfigDuration(name Value) Value       { return scalar(name, decodeDuration) }
func ConfigByteSize(name Value) Value       { return scalar(name, decodeByteSize) }
func ConfigURL(name Value) Value            { return scalar(name, decodeURL) }
func ConfigDate(name Value) Value           { return scalar(name, decodeDate) }
func ConfigLogLevel(name Value) Value       { return scalar(name, literals(stringsOf(LogLevelValues))) }
func ConfigLiteral(literal, name Value) Value {
	return scalar(name, literals([]string{literal.(string)}))
}
func ConfigLiterals(options, name Value) Value { return scalar(name, literals(stringsOf(options))) }

// ConfigRedacted is `Config.redacted`: an effect, as only a server reads a secret.
func ConfigRedacted(_ *Fiber, name Value) Value {
	return ConfigMap(Fn(func(_ *Fiber, v Value) Value { return RedactedMake(v) }), ConfigString(name))
}

// schemaDecoder reads the text of a value of a scalar schema, as its
// string tree form does.
func schemaDecoder(s *Schema) decoder {
	switch s.resolved().kind {
	case kindNumber:
		return decodeNumber
	case kindInt:
		return decodeInt
	case kindBool:
		return func(text string) (Value, string) {
			if text != "true" && text != "false" {
				return nil, `Expected "true" | "false"`
			}
			return text == "true", ""
		}
	case kindBigint:
		return func(text string) (Value, string) {
			n, ok := new(big.Int).SetString(text, 10)
			if !ok {
				return nil, "Expected a string representing a bigint"
			}
			return n, ""
		}
	}
	return decodeString
}

// ConfigSchema is the value at the path decoded by the schema: a record
// schema reads a record of the provider, a list one an array.
// shortcut: scalar, record and list schemas; the other string tree forms when a program reads one
func ConfigSchema(schema, name Value) Value {
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		path := append(append([]Value{}, prefix...), name)
		return decodeAt(fb, p, sch(schema), path)
	}}
}

func decodeAt(fb *Fiber, p provider, s *Schema, path []Value) (Value, Value) {
	node := load(fb, p, path)
	switch s.resolved().kind {
	case kindRecord:
		r := s.resolved()
		if node == nil || node.kind != cfgRecord {
			return nil, schemaError(path, "Expected object")
		}
		fields := make([]Value, len(r.labels))
		for i, label := range r.labels {
			v, missing := decodeAt(fb, p, r.items[i], append(append([]Value{}, path...), label))
			if missing != nil {
				return Fail(missing), nil
			}
			fields[i] = v
		}
		return shaped(r.keys, r.pos, fields), nil
	case kindList:
		if node == nil || node.kind != cfgArray {
			return nil, schemaError(path, "Expected array")
		}
		return decodeArray(fb, p, s.resolved().item, path, node.length)
	}
	if node == nil || node.value == nil {
		return nil, schemaError(path, "Expected string")
	}
	v, issue := schemaDecoder(s)(*node.value)
	if issue != "" {
		return Fail(schemaError(path, issue)), nil
	}
	return v, nil
}

func decodeArray(fb *Fiber, p provider, item *Schema, path []Value, length int) (Value, Value) {
	out := make([]Value, length)
	for i := range out {
		v, missing := decodeAt(fb, p, item, append(append([]Value{}, path...), int32(i)))
		if missing != nil {
			return Fail(missing), nil
		}
		out[i] = v
	}
	return out, nil
}

func optionOr(o Value, fallback string) string {
	if s := optionString(o); s != nil {
		return *s
	}
	return fallback
}

// ConfigArray is an array of the provider, or a value split by the separator.
func ConfigArray(options, schema, name Value) Value {
	separator := optionOr(record(options).Get("separator"), ",")
	item := sch(schema)
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		path := append(append([]Value{}, prefix...), name)
		node := load(fb, p, path)
		switch {
		case node != nil && node.value != nil:
			parts := strings.Split(*node.value, separator)
			out := make([]Value, len(parts))
			for i, part := range parts {
				v, issue := schemaDecoder(item)(part)
				if issue != "" {
					return Fail(schemaError(append(path, int32(i)), issue)), nil
				}
				out[i] = v
			}
			return out, nil
		case node != nil && node.kind == cfgArray:
			return decodeArray(fb, p, item, path, node.length)
		}
		return nil, schemaError(path, "Expected string | array")
	}}
}

// ConfigRecord is a record of the provider, or a value of `key=value`
// pairs, as pairs.
func ConfigRecord(options, schema, name Value) Value {
	o := record(options)
	separator := optionOr(o.Get("separator"), ",")
	kv := optionOr(o.Get("keyValueSeparator"), "=")
	item := sch(schema)
	return &config{func(fb *Fiber, p provider, prefix []Value) (Value, Value) {
		path := append(append([]Value{}, prefix...), name)
		node := load(fb, p, path)
		var out []Value
		switch {
		case node != nil && node.kind == cfgRecord:
			for _, k := range node.keys {
				v, missing := decodeAt(fb, p, item, append(append([]Value{}, path...), k))
				if missing != nil {
					return Fail(missing), nil
				}
				out = append(out, []Value{k, v})
			}
			return out, nil
		case node != nil && node.value != nil:
			for _, pair := range strings.Split(*node.value, separator) {
				k, text, ok := strings.Cut(pair, kv)
				if !ok {
					return Fail(schemaError(path, "Expected a key-value pair")), nil
				}
				v, issue := schemaDecoder(item)(text)
				if issue != "" {
					return Fail(schemaError(append(path, k), issue)), nil
				}
				out = append(out, []Value{k, v})
			}
			return out, nil
		}
		return nil, schemaError(path, "Expected object | string")
	}}
}
