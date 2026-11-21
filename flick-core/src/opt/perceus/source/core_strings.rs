//! Audited `flick:String` and `flick:rt` string foreign adapters beyond `strings.rs`.

use flick_ir::memory::StringIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::{BinOp, Code, Env, Intrinsic, Lit, Local, Lower, LowerError, Span};

impl Lower<'_> {
    pub(super) fn core_string_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        if from == "flick:String" && bind == "isNonEmpty" && value.stages.len() == 1 {
            let b = ExprBuilder::new(span, "$str");
            let expr = b.lam(
                &["s"],
                b.bin(
                    BinOp::NotEq,
                    b.loc("s"),
                    b.at(super::ExprKind::Lit(Lit::String(String::new()))),
                ),
            );
            return Some(self.expr(&expr, &Env::new(), block));
        }
        let kind = match (from, bind) {
            ("flick:String", "trim") => StringIntrinsic::Trim,
            ("flick:String", "trimStart") => StringIntrinsic::TrimStart,
            ("flick:String", "trimEnd") => StringIntrinsic::TrimEnd,
            ("flick:String", "split") => StringIntrinsic::Split,
            ("flick:String", "endsWith") => StringIntrinsic::EndsWith,
            ("flick:String", "indexOf") => StringIntrinsic::IndexOf,
            ("flick:String", "lastIndexOf") => StringIntrinsic::LastIndexOf,
            ("flick:String", "toUpperCase") => StringIntrinsic::ToUpper,
            ("flick:String", "toLowerCase") => StringIntrinsic::ToLower,
            ("flick:String", "toLocaleUpperCase") => StringIntrinsic::ToLocaleUpper,
            ("flick:String", "toLocaleLowerCase") => StringIntrinsic::ToLocaleLower,
            ("flick:String", "capitalize") => StringIntrinsic::Capitalize,
            ("flick:String", "uncapitalize") => StringIntrinsic::Uncapitalize,
            ("flick:String", "slice") => StringIntrinsic::Slice,
            ("flick:String", "substring") => StringIntrinsic::Substring,
            ("flick:String", "takeLeft") => StringIntrinsic::TakeLeft,
            ("flick:String", "takeRight") => StringIntrinsic::TakeRight,
            ("flick:String", "at") => StringIntrinsic::At,
            ("flick:String", "charAt") => StringIntrinsic::CharAt,
            ("flick:String", "charCodeAt") => StringIntrinsic::CharCodeAt,
            ("flick:String", "codePointAt") => StringIntrinsic::CodePointAt,
            ("flick:String", "flickNormalize") => StringIntrinsic::Normalize,
            ("flick:String", "padStart") => StringIntrinsic::PadStart,
            ("flick:String", "padEnd") => StringIntrinsic::PadEnd,
            ("flick:String", "repeat") => StringIntrinsic::Repeat,
            ("flick:rt", "stringReplaceAll") => StringIntrinsic::ReplaceAll,
            ("flick:String", "flickLines") => StringIntrinsic::Lines,
            ("flick:String", "flickLinesWithSeparators") => StringIntrinsic::LinesWithSeparators,
            ("flick:String", "stripMargin") => StringIntrinsic::StripMargin,
            ("flick:String", "stripMarginWith") => StringIntrinsic::StripMarginWith,
            ("flick:String", "snakeToCamel") => StringIntrinsic::SnakeToCamel,
            ("flick:String", "snakeToPascal") => StringIntrinsic::SnakeToPascal,
            ("flick:String", "snakeToKebab") => StringIntrinsic::SnakeToKebab,
            ("flick:String", "camelToSnake") => StringIntrinsic::CamelToSnake,
            ("flick:String", "pascalToSnake") => StringIntrinsic::PascalToSnake,
            ("flick:String", "kebabToSnake") => StringIntrinsic::KebabToSnake,
            ("flick:String", "noCase") => StringIntrinsic::NoCase,
            ("flick:String", "pascalCase") => StringIntrinsic::PascalCase,
            ("flick:String", "camelCase") => StringIntrinsic::CamelCase,
            ("flick:String", "constantCase") => StringIntrinsic::ConstantCase,
            ("flick:String", "configCase") => StringIntrinsic::ConfigCase,
            ("flick:String", "kebabCase") => StringIntrinsic::KebabCase,
            ("flick:String", "snakeCase") => StringIntrinsic::SnakeCase,
            ("flick:rt", "stringChars") => StringIntrinsic::Chars,
            _ => return None,
        };
        if kind.arity() != value.stages.len() {
            return None;
        }
        Some(self.intrinsic(Intrinsic::String(kind), false, vec![], block, span))
    }
}
