package flickrt

import (
	"math/big"
	"testing"
)

func TestSqlTemplateSplicesATemplateUsedAsASubquery(t *testing.T) {
	accounts := Rec("name", "a$1", "columns", []Value{SqlColumn("owner", "bigint", false, Unit)})
	users := Rec("name", "users", "columns", []Value{SqlColumn("name", "text", false, Unit)})
	owners := SqlTemplate(Rec(
		"text", `SELECT "a"."owner" AS "owner" FROM "a$1" AS "a" WHERE ("a"."owner" > $1)`,
		"tables", []Value{accounts},
		"params", []Value{Rec("name", "$min", "nullable", false, "table", int32(0), "column", "owner", "value", big.NewInt(3))},
		"columns", []Value{},
	))
	outer := record(SqlTemplate(Rec(
		"text", `SELECT 1 FROM "users" AS "u" WHERE (("u"."name" = $1) AND ("u"."id" IN ($2))) LIMIT $3`,
		"tables", []Value{users},
		"params", []Value{
			Rec("name", "name", "nullable", false, "table", int32(0), "column", "name"),
			Rec("name", "$owners", "nullable", false, "sql_kind", "template", "value", owners),
			Rec("name", "n", "nullable", false, "sql_kind", "number"),
		},
		"columns", []Value{},
	)))
	want := `SELECT 1 FROM "users" AS "u" WHERE (("u"."name" = $1) AND ("u"."id" IN (SELECT "a"."owner" AS "owner" FROM "a$1" AS "a" WHERE ("a"."owner" > $2)))) LIMIT $3`
	if got := outer.Get("text"); got != want {
		t.Fatalf("text:\n got %v\nwant %v", got, want)
	}
	if n := len(List(outer.Get("tables"))); n != 2 {
		t.Fatalf("tables: %d", n)
	}
	q := SqlBind(outer, Rec("name", "Ada", "n", int32(5))).(*sqlQuery)
	args := sqlArgs(nil, q)
	var got []string
	for _, a := range args {
		got = append(got, *a)
	}
	if len(got) != 3 || got[0] != "Ada" || got[1] != "3" || got[2] != "5" {
		t.Fatalf("args: %v", got)
	}
}
