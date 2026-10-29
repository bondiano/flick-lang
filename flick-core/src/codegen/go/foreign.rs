//! `foreign` bindings: each JS export the target supports has a Go function
//! in the runtime that takes the binding's dictionaries and arguments at
//! once and works on Flick values (callbacks stay curried, `Option` stays
//! an `Option`). A binding the program reaches without one is reported.

use super::{Gen, names, param_list};
use crate::ir::Foreign;
use crate::source::Span;

const RUNTIME: &str = "@flick-lang/runtime";

/// The Go function of the binding `name` of `module` to `export` of
/// `from`; a JS global, which may mean different things for different
/// types, is looked up by the binding itself.
fn lookup(module: &str, name: &str, from: Option<&str>, export: &str) -> Option<&'static str> {
    let Some(from) = from else {
        return global(module, name);
    };
    let found = match (from, export) {
        (RUNTIME, "equals") => "flickrt.Equals",
        (RUNTIME, "compare") => "flickrt.Compare",
        (RUNTIME, "hash") => "flickrt.Hash",
        (RUNTIME, "show") => "flickrt.Show",
        (RUNTIME, "identity") => "flickrt.Identity",
        (RUNTIME, "listEquals") => "flickrt.ListEquals",
        (RUNTIME, "listCompare") => "flickrt.ListCompare",
        (RUNTIME, "listHash") => "flickrt.ListHash",
        (RUNTIME, "listShow") => "flickrt.ListShow",
        (RUNTIME, "stdoutLine") => "flickrt.StdoutLine",
        (RUNTIME, "stderrLine") => "flickrt.StderrLine",
        (RUNTIME, "stdoutWrite") => "flickrt.StdoutWrite",
        (RUNTIME, "platformArgs") => "flickrt.PlatformArgs",
        (RUNTIME, "platformEnv") => "flickrt.PlatformEnv",
        (RUNTIME, "flickTodo") => "flickrt.Todo",
        (RUNTIME, "flickAssert") => "flickrt.Assert",
        (RUNTIME, "flickEcho") => "flickrt.Echo",
        (RUNTIME, "refMake") => "flickrt.RefMake",
        (RUNTIME, "refGet") => "flickrt.RefGet",
        (RUNTIME, "refSame") => "flickrt.RefSame",
        (RUNTIME, "refSet") => "flickrt.RefSet",
        (RUNTIME, "refModify") => "flickrt.RefModify",
        (RUNTIME, "stringFromNumber") => "flickrt.StringFromNumber",
        (RUNTIME, "intParse") => "flickrt.IntParse",
        (RUNTIME, "intFromFloat") => "flickrt.IntFromFloat",
        (RUNTIME, "intTruncate") => "flickrt.IntTruncate",
        (RUNTIME, "intAbs") => "flickrt.IntAbs",
        (RUNTIME, "intCheckedAdd") => "flickrt.IntCheckedAdd",
        (RUNTIME, "intCheckedSub") => "flickrt.IntCheckedSub",
        (RUNTIME, "intCheckedMul") => "flickrt.IntCheckedMul",
        (RUNTIME, "intCheckedDiv") => "flickrt.IntCheckedDiv",
        (RUNTIME, "intCheckedRem") => "flickrt.IntCheckedRem",
        (RUNTIME, "intUnsafeDiv") => "flickrt.IntUnsafeDiv",
        (RUNTIME, "intUnsafeRem") => "flickrt.IntUnsafeRem",
        (RUNTIME, "bitAnd") => "flickrt.BitAnd",
        (RUNTIME, "bitOr") => "flickrt.BitOr",
        (RUNTIME, "bitXor") => "flickrt.BitXor",
        (RUNTIME, "bitNot") => "flickrt.BitNot",
        (RUNTIME, "shiftLeft") => "flickrt.ShiftLeft",
        (RUNTIME, "shiftRight") => "flickrt.ShiftRight",
        (RUNTIME, "shiftRightUnsignedInt") => "flickrt.ShiftRightUnsignedInt",
        (RUNTIME, "numberStrictEqual") => "flickrt.NumberStrictEqual",
        (RUNTIME, "numberSameValue") => "flickrt.NumberSameValue",
        (RUNTIME, "listMap") => "flickrt.ListMap",
        (RUNTIME, "listMapIndexed") => "flickrt.ListMapIndexed",
        (RUNTIME, "listEachEffect") => "flickrt.ListEach",
        (RUNTIME, "listFold") => "flickrt.ListFold",
        (RUNTIME, "listFoldRight") => "flickrt.ListFoldRight",
        (RUNTIME, "listFilter") => "flickrt.ListFilter",
        (RUNTIME, "listFilterMap") => "flickrt.ListFilterMap",
        (RUNTIME, "listFlatMap") => "flickrt.ListFlatMap",
        (RUNTIME, "listMap2") => "flickrt.ListMap2",
        (RUNTIME, "optionMap2") => "flickrt.OptionMap2",
        (RUNTIME, "resultMap2") => "flickrt.ResultMap2",
        (RUNTIME, "listFind") => "flickrt.ListFind",
        (RUNTIME, "listFindIndex") => "flickrt.ListFindIndex",
        (RUNTIME, "listAny") => "flickrt.ListAny",
        (RUNTIME, "listAll") => "flickrt.ListAll",
        (RUNTIME, "listPartition") => "flickrt.ListPartition",
        (RUNTIME, "listSortBy") => "flickrt.ListSortBy",
        (RUNTIME, "listLength") => "flickrt.ListLength",
        (RUNTIME, "listGet") => "flickrt.ListGet",
        (RUNTIME, "listSet") => "flickrt.ListSet",
        (RUNTIME, "listSingleton") => "flickrt.ListSingleton",
        (RUNTIME, "listRepeat") => "flickrt.ListRepeat",
        (RUNTIME, "listRange") => "flickrt.ListRange",
        (RUNTIME, "listInitialize") => "flickrt.ListInitialize",
        (RUNTIME, "listReverse") => "flickrt.ListReverse",
        (RUNTIME, "listConcat") => "flickrt.ListConcatAll",
        (RUNTIME, "listTake") => "flickrt.ListTake",
        (RUNTIME, "listDrop") => "flickrt.ListDrop",
        (RUNTIME, "listSlice") => "flickrt.ListSlice",
        (RUNTIME, "listZip") => "flickrt.ListZip",
        (RUNTIME, "listUnzip") => "flickrt.ListUnzip",
        _ => return None,
    };
    Some(found)
}

/// The Go value or function of a binding to a JS global.
fn global(module: &str, name: &str) -> Option<&'static str> {
    let found = match (module, name) {
        ("Int", "to_float") => "flickrt.IntToFloat",
        ("Numeric", "bigint_of") => "flickrt.BigIntOfInt",
        ("Numeric", "float_of_bigint") => "flickrt.FloatOfBigInt",
        ("Float", "pi") => "flickrt.Pi",
        ("Float", "infinity") => "flickrt.Inf",
        ("Float", "nan") => "flickrt.NaN",
        ("Float", "is_nan") => "flickrt.FloatIsNaN",
        ("Float", "is_finite") => "flickrt.FloatIsFinite",
        ("Float", "is_integer") => "flickrt.FloatIsInteger",
        ("Float", "is_safe_integer") => "flickrt.FloatIsSafeInteger",
        ("Float", "abs") => "flickrt.FloatAbs",
        ("Float", "floor") => "flickrt.FloatFloor",
        ("Float", "ceil") => "flickrt.FloatCeil",
        ("Float", "round") => "flickrt.FloatRound",
        ("Float", "trunc") => "flickrt.FloatTrunc",
        ("Float", "sqrt") => "flickrt.FloatSqrt",
        _ => return None,
    };
    Some(found)
}

impl Gen<'_> {
    /// The Go function or variable of a binding, `None` for one without a
    /// Go form, which is reported.
    pub(super) fn foreign(&mut self, f: &Foreign, span: Span) -> Option<String> {
        let Some(target) = lookup(self.module, &f.name, f.module.as_deref(), &f.export) else {
            let from = f
                .module
                .as_deref()
                .map_or_else(|| "global".to_owned(), |m| format!("from `{m}`"));
            self.errors.push((
                format!(
                    "`{}.{}` (foreign `{}` {from}) is not available on the go target",
                    self.module, f.name, f.export
                ),
                Some(span),
            ));
            return None;
        };
        let taken = f.info.stages_taken();
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return Some(format!("var {var} flickrt.Value = {target}"));
        }
        let heads: Vec<String> = (0..f.dicts)
            .map(|i| format!("d{i}"))
            .chain((0..taken).map(|i| format!("a{i}")))
            .collect();
        let args = &heads[..f.dicts + f.arity.min(taken)];
        Some(format!(
            "func {}({}) flickrt.Value {{\n\treturn {target}({})\n}}",
            names::func(self.module, &f.name),
            param_list(&heads),
            args.join(", ")
        ))
    }
}
