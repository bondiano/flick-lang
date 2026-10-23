//! SQL dialects as data: what `[sql] dialect` in `flick.toml` selects. The
//! checker reads column types and the template tag from the dialect, and the
//! editor offers its types and functions, so a new dialect is a new value.

/// A SQL type a column may have.
#[derive(Debug, PartialEq, Eq)]
pub struct ColumnType {
    /// The words of the type, as a table writes them.
    pub words: &'static str,
    /// How the runtime passes and reads values of it.
    pub kind: &'static str,
    /// The Flick type of a value without a codec.
    pub flick: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Dialect {
    /// The value of `[sql] dialect`.
    pub name: &'static str,
    /// How documentation and the editor name the dialect.
    pub title: &'static str,
    /// The `Sql` type that tags its templates: `Sql.Template Pg p r`.
    pub tag: &'static str,
    pub types: &'static [ColumnType],
    /// Functions a query may call, with what they do.
    pub functions: &'static [(&'static str, &'static str)],
}

impl Dialect {
    /// The column type that `words` start with.
    pub fn column_type(&self, words: &[&str]) -> Option<&'static ColumnType> {
        self.types.iter().find(|t| {
            let n = t.words.split(' ').count();
            words.len() >= n && words[..n].join(" ") == t.words
        })
    }

    /// The Flick type name of a value of a runtime kind, without a codec.
    pub fn flick_type(&self, kind: &str) -> &'static str {
        self.types
            .iter()
            .find(|t| t.kind == kind)
            .map_or("Unknown", |t| t.flick)
    }
}

/// The checker and editor of a module checked outside a project use it.
impl Default for &'static Dialect {
    fn default() -> Self {
        &POSTGRESQL
    }
}

const fn ty(words: &'static str, kind: &'static str, flick: &'static str) -> ColumnType {
    ColumnType { words, kind, flick }
}

pub static POSTGRESQL: Dialect = Dialect {
    name: "postgresql",
    title: "PostgreSQL",
    tag: "Pg",
    types: &[
        ty("bigint", "bigint", "BigInt"),
        ty("int8", "bigint", "BigInt"),
        ty("bigserial", "bigint", "BigInt"),
        ty("integer", "number", "Number"),
        ty("int", "number", "Number"),
        ty("int4", "number", "Number"),
        ty("smallint", "number", "Number"),
        ty("int2", "number", "Number"),
        ty("serial", "number", "Number"),
        ty("real", "number", "Number"),
        ty("double precision", "number", "Number"),
        ty("float8", "number", "Number"),
        ty("numeric", "numeric", "BigDecimal"),
        ty("decimal", "numeric", "BigDecimal"),
        ty("text", "text", "String"),
        ty("varchar", "text", "String"),
        ty("character varying", "text", "String"),
        ty("uuid", "uuid", "String"),
        ty("boolean", "bool", "Bool"),
        ty("bool", "bool", "Bool"),
        ty("date", "date", "String"),
        ty("timestamp", "timestamp", "String"),
        ty("timestamptz", "timestamptz", "String"),
        ty("jsonb", "json", "Json"),
        ty("json", "json", "Json"),
    ],
    functions: &[
        ("is_null", "Tests whether a SQL value is NULL."),
        ("is_not_null", "Tests whether a SQL value is not NULL."),
        (
            "is_not_distinct_from",
            "Null-safe equality of two SQL values.",
        ),
        ("coalesce", "The first non-NULL value."),
        ("count", "Counts non-NULL values in a group."),
        ("count_all", "Counts all rows in a group; result: BigInt."),
        ("sum", "The sum of a group's values."),
        ("avg", "The average of a group's values."),
        ("min", "The minimum of a group's values."),
        ("max", "The maximum of a group's values."),
        ("exists", "Tests whether a subquery returns any rows."),
        ("is_in", "Tests membership in a single-column subquery."),
    ],
};

/// The names `[sql] dialect` accepts, with the dialect when it is supported.
pub static DIALECTS: [(&str, Option<&Dialect>); 5] = [
    ("postgresql", Some(&POSTGRESQL)),
    ("mysql", None),
    ("sqlite", None),
    ("clickhouse", None),
    ("oracle", None),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_word_types_and_kinds() {
        let t = POSTGRESQL.column_type(&["double", "precision", "not", "null"]);
        assert_eq!(t.map(|t| t.flick), Some("Number"));
        assert!(POSTGRESQL.column_type(&["double"]).is_none());
        assert_eq!(POSTGRESQL.flick_type("numeric"), "BigDecimal");
        assert_eq!(DIALECTS[0].1, Some(&POSTGRESQL));
    }
}
