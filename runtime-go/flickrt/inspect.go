package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

// A port of Node's `util.inspect` with its default options (depth 2,
// breakLength 80, compact 3), for the values of Flick as the JS target
// holds them: a list or a tuple is an array, a record an object, a variant
// value an object of its `_tag` and fields. With `custom`, as `console.log`
// shows a value, Effect's Option and Result show their `toJSON`; without,
// as `console.dir`, their own fields.
//
// shortcut: values of other runtime types (durations, dates, maps, functions) show as `show` or `[Function (anonymous)]`, where Node shows their classes; port their inspection when a program writes them to the console

const inspectCompact = 3

type inspectCtx struct {
	custom       bool
	depth        int // how deep objects show; deeper ones are `[Object]`
	maxList      int // how many elements of an array show
	breakLength  int
	indent       int
	currentDepth int
}

// inspect is a value as `console.dir` shows it.
func inspect(v Value) string {
	return (&inspectCtx{depth: 2, maxList: 100, breakLength: 80}).value(v, 0)
}

// inspectCustom is a value as `console.log` shows it.
func inspectCustom(v Value) string {
	return (&inspectCtx{custom: true, depth: 2, maxList: 100, breakLength: 80}).value(v, 0)
}

func (c *inspectCtx) value(v Value, depth int) string {
	switch x := v.(type) {
	case string:
		return c.str(x)
	case int32:
		return strconv.Itoa(int(x))
	case float64:
		if x == 0 && math.Signbit(x) {
			return "-0"
		}
		return ShowNumber(x)
	case *big.Int:
		return x.String() + "n"
	case bool:
		return strconv.FormatBool(x)
	case UnitType:
		return "undefined"
	case []Value:
		return c.raw(depth, nil, x, true)
	case *Record:
		return c.raw(depth, x.keys, x.vals, false)
	case *Data:
		keys, vals := c.dataFields(x)
		return c.raw(depth, keys, vals, false)
	case Fn, Poly:
		return "[Function (anonymous)]"
	}
	return Show(v).(string)
}

// dataFields are the keys and values Node sees of a variant value.
func (c *inspectCtx) dataFields(d *Data) ([]string, []Value) {
	switch d.Tag {
	case "Some", "None":
		if c.custom {
			return withTag("Option", d)
		}
	case "Success", "Failure":
		if c.custom {
			return withTag("Result", d)
		}
	default:
		keys := []string{"_tag"}
		vals := []Value{d.Tag}
		for i, f := range d.Fields {
			keys = append(keys, nativeField(d.Tag, i))
			vals = append(vals, f)
		}
		return keys, vals
	}
	keys := make([]string, len(d.Fields))
	for i := range d.Fields {
		keys[i] = nativeField(d.Tag, i)
	}
	return keys, d.Fields
}

// withTag is the `toJSON` of Effect's Option and Result.
func withTag(id string, d *Data) ([]string, []Value) {
	keys := []string{"_id", "_tag"}
	vals := []Value{id, d.Tag}
	if len(d.Fields) > 0 {
		field := "value"
		if d.Tag == "Failure" {
			field = "failure"
		}
		keys = append(keys, field)
		vals = append(vals, d.Fields[0])
	}
	return keys, vals
}

var identifierKey = regexp.MustCompile(`^[a-zA-Z_][a-zA-Z_0-9]*$`)

// raw is Node's `formatRaw` of an array or an object.
func (c *inspectCtx) raw(depth int, keys []string, vals []Value, array bool) string {
	open, closing := "{", "}"
	if array {
		open, closing = "[", "]"
	}
	if len(vals) == 0 {
		return open + closing
	}
	if depth > c.depth {
		if array {
			return "[Array]"
		}
		return "[Object]"
	}
	depth++
	c.currentDepth = depth
	n := len(vals)
	if array {
		n = min(n, c.maxList)
	}
	output := make([]string, 0, n+1)
	for i := 0; i < n; i++ {
		c.indent += 2
		s := c.value(vals[i], depth)
		c.indent -= 2
		if !array {
			name := keys[i]
			if !identifierKey.MatchString(name) {
				name = strEscape(name)
			}
			s = name + ": " + s
		}
		output = append(output, s)
	}
	if more := len(vals) - n; more > 0 {
		output = append(output, "... "+strconv.Itoa(more)+" more item"+plural(more))
	}
	return c.reduce(output, open, closing, array, depth, vals)
}

func plural(n int) string {
	if n > 1 {
		return "s"
	}
	return ""
}

// reduce is Node's `reduceToSingleString`: on one line when the entries
// fit and the value nests at most `compact` levels deep, else one entry a
// line, short array entries grouped in columns.
func (c *inspectCtx) reduce(output []string, open, closing string, array bool, depth int, vals []Value) string {
	entries := len(output)
	if array && entries > 6 {
		output = groupArrayElements(c, output, vals)
	}
	if c.currentDepth-depth < inspectCompact && entries == len(output) {
		start := len(output) + c.indent + len(open) + 10
		if belowBreakLength(output, start, c.breakLength) {
			joined := strings.Join(output, ", ")
			if !strings.Contains(joined, "\n") {
				return open + " " + joined + " " + closing
			}
		}
	}
	indentation := "\n" + strings.Repeat(" ", c.indent)
	return open + indentation + "  " + strings.Join(output, ","+indentation+"  ") + indentation + closing
}

func belowBreakLength(output []string, start, breakLength int) bool {
	total := len(output) + start
	if total+len(output) > breakLength {
		return false
	}
	for _, o := range output {
		total += utf16Len(o)
		if total > breakLength {
			return false
		}
	}
	return true
}

// groupArrayElements is Node's grouping of many short array entries into
// aligned columns.
func groupArrayElements(c *inspectCtx, output []string, vals []Value) []string {
	totalLength, maxLength := 0, 0
	outputLength := len(output)
	if len(vals) > c.maxList {
		outputLength--
	}
	const separatorSpace = 2
	dataLen := make([]int, outputLength)
	for i := 0; i < outputLength; i++ {
		l := utf16Len(output[i])
		dataLen[i] = l
		totalLength += l + separatorSpace
		maxLength = max(maxLength, l)
	}
	actualMax := maxLength + separatorSpace
	if actualMax*3+c.indent >= c.breakLength ||
		(float64(totalLength)/float64(actualMax) <= 5 && maxLength > 6) {
		return output
	}
	averageBias := math.Sqrt(float64(actualMax) - float64(totalLength)/float64(len(output)))
	biasedMax := math.Max(float64(actualMax)-3-averageBias, 1)
	columns := min(
		int(math.Floor(math.Sqrt(2.5*biasedMax*float64(outputLength))/biasedMax+0.5)),
		(c.breakLength-c.indent)/actualMax,
		inspectCompact*4,
		15,
	)
	if columns <= 1 {
		return output
	}
	maxLineLength := make([]int, columns)
	for i := 0; i < columns; i++ {
		lineLength := 0
		for j := i; j < outputLength; j += columns {
			lineLength = max(lineLength, dataLen[j])
		}
		maxLineLength[i] = lineLength + separatorSpace
	}
	padStart := true
	for _, v := range vals[:min(len(vals), len(output))] {
		switch v.(type) {
		case int32, float64, *big.Int:
		default:
			padStart = false
		}
	}
	var grouped []string
	for i := 0; i < outputLength; i += columns {
		end := min(i+columns, outputLength)
		var line strings.Builder
		j := i
		for ; j < end-1; j++ {
			line.WriteString(padTo(output[j]+", ", maxLineLength[j-i], padStart))
		}
		if padStart {
			line.WriteString(padTo(output[j], maxLineLength[j-i]-separatorSpace, true))
		} else {
			line.WriteString(output[j])
		}
		grouped = append(grouped, line.String())
	}
	if len(vals) > c.maxList {
		grouped = append(grouped, output[outputLength])
	}
	return grouped
}

// padTo pads the text with spaces to the width in UTF-16 units, as
// `padStart` (or `padEnd`) does.
func padTo(s string, width int, start bool) string {
	n := width - utf16Len(s)
	if n <= 0 {
		return s
	}
	if start {
		return strings.Repeat(" ", n) + s
	}
	return s + strings.Repeat(" ", n)
}

// str is a string as Node quotes it, split after its line breaks when it
// is too long for the line.
func (c *inspectCtx) str(s string) string {
	n := utf16Len(s)
	if n > 16 && n > c.breakLength-c.indent-4 && strings.Contains(s, "\n") {
		var lines []string
		for rest := s; rest != ""; {
			i := strings.IndexByte(rest, '\n')
			if i < 0 {
				lines = append(lines, strEscape(rest))
				break
			}
			lines = append(lines, strEscape(rest[:i+1]))
			rest = rest[i+1:]
		}
		return strings.Join(lines, " +\n"+strings.Repeat(" ", c.indent+2))
	}
	return strEscape(s)
}

// strEscape is Node's quoting of a string: single quotes unless it has
// one, control characters and lone surrogates escaped.
func strEscape(s string) string {
	quote := byte('\'')
	if strings.Contains(s, "'") {
		if !strings.Contains(s, `"`) {
			quote = '"'
		} else if !strings.Contains(s, "`") && !strings.Contains(s, "${") {
			quote = '`'
		}
	}
	var out strings.Builder
	out.WriteByte(quote)
	units := utf16Units(s)
	for i := 0; i < len(units); i++ {
		u := units[i]
		switch {
		case u == '\'' && quote == '\'':
			out.WriteString(`\'`)
		case u == '\\':
			out.WriteString(`\\`)
		case u == '\b':
			out.WriteString(`\b`)
		case u == '\t':
			out.WriteString(`\t`)
		case u == '\n':
			out.WriteString(`\n`)
		case u == '\f':
			out.WriteString(`\f`)
		case u == '\r':
			out.WriteString(`\r`)
		case u < 32 || (u > 126 && u < 160):
			out.WriteString(`\x` + strings.ToUpper(strconv.FormatInt(int64(u)|0x100, 16)[1:]))
		case u >= 0xd800 && u <= 0xdbff && i+1 < len(units) && units[i+1] >= 0xdc00 && units[i+1] <= 0xdfff:
			out.WriteString(fromUnits(units[i : i+2]))
			i++
		case u >= 0xd800 && u <= 0xdfff:
			out.WriteString(`\u` + strconv.FormatInt(int64(u), 16))
		default:
			out.WriteString(fromUnits(units[i : i+1]))
		}
	}
	out.WriteByte(quote)
	return out.String()
}
