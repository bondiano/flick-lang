package flickrt

// `Sql`: tables and templates as the compiler emits them, binding without
// I/O, raw SQL with identifiers on their own channel, and how values cross
// the driver: parameters go as text, never into the SQL text; results are
// read as text, checked against the column's type OID and decoded by the
// kind the compiler settled, then by the column's codec.

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

var (
	sqlEncodeErrorTag = Tag("Sql.EncodeError.EncodeError")
	sqlDecodeErrorTag = Tag("Sql.DecodeError.DecodeError")
	sqlDbErrorTag     = Tag("Sql.DbError.DbError")
)

func sqlEncodeError(parameter, message string) Value {
	return NewData(sqlEncodeErrorTag, Rec("parameter", parameter, "message", message))
}

func sqlDecodeError(column, message string) Value {
	return NewData(sqlDecodeErrorTag, Rec("column", column, "message", message))
}

// sqlQuery is a template with its parameters bound, or raw SQL with its
// placeholders' values and the decoder of its rows.
type sqlQuery struct {
	template *Record // nil for raw SQL
	values   *Record
	text     string
	params   []rawPiece
	decoder  Value
}

// rawPiece is text of raw SQL, or a value passed as a placeholder.
type rawPiece struct {
	text  string
	codec Value // nil for text
	value Value
}

func SqlTable(table Value) Value       { return table }
func SqlTemplate(template Value) Value { return template }

// SqlCodec is `Sql.codec decode encode`, over the value of the column's
// SQL type.
func SqlCodec(decode, encode Value) Value {
	return CodecTransformResult(decode, encode, sch(SchemaUnknown).jsonOf())
}

func SqlBind(template, values Value) Value {
	t := record(template)
	return &sqlQuery{template: t, values: record(values), text: t.Get("text").(string)}
}

func SqlText(q Value) Value         { return q.(*sqlQuery).text }
func SqlTemplateText(t Value) Value { return record(t).Get("text") }

func SqlParameterCount(q Value) Value {
	query := q.(*sqlQuery)
	if query.template != nil {
		return int32(len(List(query.template.Get("params"))))
	}
	return int32(len(query.params))
}

var sqlIdentifierPattern = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)

// SqlIdentifier is a checked identifier, or None.
func SqlIdentifier(name Value) Value {
	n := name.(string)
	if sqlIdentifierPattern.MatchString(n) && len(n) <= 63 {
		return Some(n)
	}
	return None
}

func SqlRawText(text Value) Value { return rawPiece{text: text.(string)} }

func SqlRawIdent(name Value) Value {
	return rawPiece{text: `"` + strings.ReplaceAll(name.(string), `"`, `""`) + `"`}
}

func SqlRawParam(codec, value Value) Value { return rawPiece{codec: codec, value: value} }

// SqlRawQuery joins the pieces; each parameter becomes the next placeholder.
func SqlRawQuery(pieces, decoder Value) Value {
	q := &sqlQuery{decoder: decoder}
	var text strings.Builder
	for _, p := range List(pieces) {
		piece := p.(rawPiece)
		if piece.codec == nil {
			text.WriteString(piece.text)
			continue
		}
		q.params = append(q.params, piece)
		text.WriteString("$" + strconv.Itoa(len(q.params)))
	}
	q.text = text.String()
	return q
}

// sqlOIDs are the type OIDs each kind accepts.
var sqlOIDs = map[string][]uint32{
	"number":      {21, 23, 26, 700, 701},
	"bigint":      {20},
	"numeric":     {1700},
	"text":        {18, 19, 25, 1042, 1043},
	"uuid":        {2950},
	"bool":        {16},
	"date":        {1082},
	"timestamp":   {1114},
	"timestamptz": {1184},
	"json":        {114, 3802},
}

func acceptsOID(kind string, oid uint32) bool {
	for _, o := range sqlOIDs[kind] {
		if o == oid {
			return true
		}
	}
	return false
}

// sqlColumn is a column of a `sql table`, or what a value of a template
// says of itself.
type sqlColumn struct {
	name     string
	kind     string
	nullable bool
	codec    Value // nil without one
}

// encodeKind is a value of a kind as the driver takes it: text, or nil.
func encodeKind(kind string, v Value) (*string, string) {
	text := func(s string) (*string, string) { return &s, "" }
	switch kind {
	case "number":
		x, ok := number(v)
		if !ok || math.IsNaN(x) || math.IsInf(x, 0) {
			return nil, StringFromAny(v) + " is not a finite number"
		}
		return text(StringFromNumber(v).(string))
	case "bigint":
		n, ok := v.(*big.Int)
		if !ok {
			return nil, "not a BigInt"
		}
		return text(n.String())
	case "numeric":
		d, ok := v.(*Decimal)
		if !ok {
			return nil, "not a BigDecimal"
		}
		return text(DecimalToString(d).(string))
	case "bool":
		b, ok := v.(bool)
		if !ok {
			return nil, "not a Bool"
		}
		return text(strconv.FormatBool(b))
	case "json":
		return text(JsonStringify(v).(string))
	}
	s, ok := v.(string)
	if !ok {
		return nil, "not a String"
	}
	return text(s)
}

var sqlInteger = regexp.MustCompile(`^-?\d+$`)

// decodeKind is a value of a kind read from its text.
func decodeKind(kind, text string) (Value, string) {
	switch kind {
	case "number":
		x, ok := pgNumber(text)
		if !ok {
			return nil, text + " is not a number"
		}
		return x, ""
	case "bigint":
		if !sqlInteger.MatchString(text) {
			return nil, text + " is not an integer"
		}
		n, _ := new(big.Int).SetString(text, 10)
		return n, ""
	case "numeric":
		d, ok := parseDecimal(text)
		if !ok {
			return nil, text + " is not a decimal"
		}
		return d, ""
	case "bool":
		if text != "t" && text != "f" {
			return nil, text + " is not a boolean"
		}
		return text == "t", ""
	case "json":
		r := JsonParse(text).(*Data)
		if r.Tag != "Success" {
			return nil, "not JSON"
		}
		return r.Fields[0], ""
	}
	return text, ""
}

// pgNumber reads a number as PostgreSQL writes one: decimal digits,
// `NaN`, `Infinity` or `-Infinity`.
func pgNumber(text string) (float64, bool) {
	switch text {
	case "NaN":
		return math.NaN(), true
	case "Infinity":
		return math.Inf(1), true
	case "-Infinity":
		return math.Inf(-1), true
	}
	x, err := strconv.ParseFloat(text, 64)
	return x, err == nil
}

func issuesMessage(issues []issue) string {
	parts := make([]string, len(issues))
	for i, is := range issues {
		path := is.path
		if path == "" {
			path = "(root)"
		}
		parts[i] = path + ": " + is.message
	}
	return strings.Join(parts, "; ")
}

// encodeColumn encodes the value through the column's codec, then by its kind.
func encodeColumn(fb *Fiber, c sqlColumn, v Value) (*string, string) {
	if c.codec != nil {
		encoded, issues := codecOf(c.codec).encode(fb, v)
		if issues != nil {
			return nil, issuesMessage(issues)
		}
		v = encoded
	}
	return encodeKind(c.kind, v)
}

// decodeColumn decodes text by the column's kind, then through its codec.
func decodeColumn(fb *Fiber, c sqlColumn, text string) (Value, string) {
	v, problem := decodeKind(c.kind, text)
	if problem != "" || c.codec == nil {
		return v, problem
	}
	decoded, issues := codecOf(c.codec).decode(fb, v)
	if issues != nil {
		return nil, issuesMessage(issues)
	}
	return decoded, ""
}

// columnOf is the column a template's value reads or writes.
func columnOf(template *Record, v *Record) sqlColumn {
	name, nullable := v.Get("name").(string), v.Get("nullable").(bool)
	own := sqlColumn{name: name, kind: "text", nullable: nullable}
	if v.index("table") < 0 {
		if v.index("kind") >= 0 {
			own.kind = v.Get("kind").(string)
		}
		return own
	}
	at, _ := number(v.Get("table"))
	table := record(List(template.Get("tables"))[int(at)])
	for _, c := range List(table.Get("columns")) {
		col := record(c)
		if col.Get("name") == v.Get("column") {
			out := sqlColumn{name: name, kind: col.Get("kind").(string), nullable: col.Get("nullable").(bool)}
			if codec := col.Get("codec"); codec != Unit {
				out.codec = codec
			}
			return out
		}
	}
	return own
}

// sqlArgs is the placeholder values of a query, each text or nil.
func sqlArgs(fb *Fiber, q *sqlQuery) []*string {
	var out []*string
	if q.template == nil {
		for i, p := range q.params {
			json, issues := codecOf(p.codec).encode(fb, p.value)
			if issues != nil {
				Fail(sqlEncodeError("$"+strconv.Itoa(i+1), "its codec refused the value"))
			}
			if json == JsonNull {
				out = append(out, nil)
				continue
			}
			text := JsonStringify(json).(string)
			if s, ok := json.(string); ok {
				text = s
			}
			out = append(out, &text)
		}
		return out
	}
	for _, pv := range List(q.template.Get("params")) {
		p := record(pv)
		name := p.Get("name").(string)
		var v Value
		if p.index("value") >= 0 {
			v = p.Get("value")
		} else {
			v = q.values.Get(name)
		}
		if p.Get("nullable").(bool) {
			o, ok := v.(*Data)
			if !ok || o.Tag != "Some" {
				out = append(out, nil)
				continue
			}
			v = o.Fields[0]
		}
		text, problem := encodeColumn(fb, columnOf(q.template, p), v)
		if problem != "" {
			Fail(sqlEncodeError(name, problem))
		}
		out = append(out, text)
	}
	return out
}
