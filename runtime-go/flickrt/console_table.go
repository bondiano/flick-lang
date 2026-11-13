package flickrt

import (
	"math"
	"strconv"
	"strings"
)

// ConsoleTable is Node's `console.table`: a box of the rows, the index
// first, a column per key of the rows (or per given property) and a
// `Values` column for rows that are not objects.
func ConsoleTable(fb *Fiber, properties, data Value) Value {
	props, only := some(properties)
	var keys []string
	columns := map[string][]string{}
	rows := List(data)
	var values []string
	hasPrimitives := false
	for i, row := range rows {
		rowKeys, rowVals, object := tableEntries(row)
		if !only && !object {
			hasPrimitives = true
			values = growColumn(values, i)
			values[i] = tableCell(row)
			continue
		}
		wanted := rowKeys
		if only {
			wanted = nil
			for _, p := range List(props) {
				wanted = append(wanted, p.(string))
			}
		}
		for _, k := range wanted {
			if _, seen := columns[k]; !seen {
				keys = append(keys, k)
			}
			column := growColumn(columns[k], i)
			column[i] = ""
			for j, rk := range rowKeys {
				if rk == k && object {
					column[i] = tableCell(rowVals[j])
				}
			}
			columns[k] = column
		}
	}
	head := append([]string{"(index)"}, keys...)
	index := make([]string, len(rows))
	for i := range rows {
		index[i] = strconv.Itoa(i)
	}
	cols := [][]string{index}
	for _, k := range keys {
		cols = append(cols, columns[k])
	}
	if hasPrimitives {
		head = append(head, "Values")
		cols = append(cols, values)
	}
	currentConsole(fb).log(renderTable(head, cols))
	return Unit
}

// growColumn is the column with room for row `i`, rows it skips empty.
func growColumn(column []string, i int) []string {
	for len(column) <= i {
		column = append(column, "")
	}
	return column
}

// tableEntries are the keys and values of a row Node lists as columns:
// those of a record or a variant value, the indices of a list.
func tableEntries(row Value) ([]string, []Value, bool) {
	switch x := row.(type) {
	case *Record:
		return x.keys, x.vals, true
	case []Value:
		keys := make([]string, len(x))
		for i := range x {
			keys[i] = strconv.Itoa(i)
		}
		return keys, x, true
	case *Data:
		keys, vals := (&inspectCtx{}).dataFields(x)
		return keys, vals, true
	}
	return nil, nil, false
}

// tableCell is a value as a cell shows it: on one line, nested objects
// and objects of more than two keys as `[Object]`.
func tableCell(v Value) string {
	depth := 0
	if keys, _, object := tableEntries(v); object {
		if _, list := v.([]Value); !list && len(keys) > 2 {
			depth = -1
		}
	}
	return (&inspectCtx{custom: true, depth: depth, maxList: 3, breakLength: math.MaxInt32}).value(v, 0)
}

// renderTable is Node's `cli_table`: left-aligned cells in a box.
func renderTable(head []string, columns [][]string) string {
	widths := make([]int, len(head))
	longest := 0
	for i, h := range head {
		widths[i] = stringWidth(h)
		longest = max(longest, len(columns[i]))
	}
	rows := make([][]string, longest)
	for j := range rows {
		rows[j] = make([]string, len(head))
		for i := range head {
			if j < len(columns[i]) {
				rows[j][i] = columns[i][j]
			}
			widths[i] = max(widths[i], stringWidth(rows[j][i]))
		}
	}
	divider := make([]string, len(widths))
	for i, w := range widths {
		divider[i] = strings.Repeat("─", w+2)
	}
	row := func(cells []string) string {
		var out strings.Builder
		out.WriteString("│ ")
		for i, cell := range cells {
			out.WriteString(cell + strings.Repeat(" ", widths[i]-stringWidth(cell)))
			if i != len(cells)-1 {
				out.WriteString(" │ ")
			}
		}
		out.WriteString(" │")
		return out.String()
	}
	var out strings.Builder
	out.WriteString("┌" + strings.Join(divider, "┬") + "┐\n" + row(head) + "\n")
	out.WriteString("├" + strings.Join(divider, "┼") + "┤\n")
	for _, r := range rows {
		out.WriteString(row(r) + "\n")
	}
	out.WriteString("└" + strings.Join(divider, "┴") + "┘")
	return out.String()
}

// stringWidth is the columns a text takes in a terminal, as Node's
// `getStringWidth` counts them: wide East Asian characters two, control
// and combining characters none.
//
// shortcut: the ranges of Node's build without ICU, where Node with ICU reads Unicode's East Asian Width; take ICU's tables when a program aligns other wide characters
func stringWidth(s string) int {
	width := 0
	for _, r := range s {
		switch {
		case isZeroWidth(r):
		case isFullWidth(r):
			width += 2
		default:
			width++
		}
	}
	return width
}

func isZeroWidth(r rune) bool {
	return r <= 0x1f || (r >= 0x7f && r <= 0x9f) || (r >= 0x300 && r <= 0x36f) ||
		(r >= 0x200b && r <= 0x200f) || (r >= 0x20d0 && r <= 0x20ff) ||
		(r >= 0xfe00 && r <= 0xfe0f) || (r >= 0xfe20 && r <= 0xfe2f) ||
		(r >= 0xe0100 && r <= 0xe01ef)
}

func isFullWidth(r rune) bool {
	return r >= 0x1100 && (r <= 0x115f || r == 0x2329 || r == 0x232a ||
		(r >= 0x2e80 && r <= 0x3247 && r != 0x303f) || (r >= 0x3250 && r <= 0x4dbf) ||
		(r >= 0x4e00 && r <= 0xa4c6) || (r >= 0xa960 && r <= 0xa97c) ||
		(r >= 0xac00 && r <= 0xd7a3) || (r >= 0xf900 && r <= 0xfaff) ||
		(r >= 0xfe10 && r <= 0xfe19) || (r >= 0xfe30 && r <= 0xfe6b) ||
		(r >= 0xff01 && r <= 0xff60) || (r >= 0xffe0 && r <= 0xffe6) ||
		(r >= 0x1b000 && r <= 0x1b001) || (r >= 0x1f200 && r <= 0x1f251) ||
		(r >= 0x1f300 && r <= 0x1f64f) || (r >= 0x20000 && r <= 0x3fffd))
}
