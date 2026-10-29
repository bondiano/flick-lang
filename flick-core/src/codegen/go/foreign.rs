//! `foreign` bindings: each JS export the target supports has a Go function
//! in the runtime that takes the binding's dictionaries and arguments at
//! once and works on Flick values (callbacks stay curried, `Option` stays
//! an `Option`). A binding the program reaches without one is reported.

use super::{Gen, names, param_list};
use crate::ir::Foreign;
use crate::source::Span;

const RUNTIME: &str = "@flick-lang/runtime";
const STRING: &str = "@flick-lang/runtime/effect/String";
const HASH_MAP: &str = "@flick-lang/runtime/effect/HashMap";
const HASH_SET: &str = "@flick-lang/runtime/effect/HashSet";

/// The Go function of each JS export the target supports: module, export.
const EXPORTS: &[(&str, &str, &str)] = &[
    (RUNTIME, "eqHashWith", "flickrt.EqHashWith"),
    (RUNTIME, "eqHashCanonical", "flickrt.EqHashCanonical"),
    (RUNTIME, "hashSetWith", "flickrt.HashSetWith"),
    (RUNTIME, "hashSetEmpty", "flickrt.HashSetEmpty"),
    (RUNTIME, "hashSetFromList", "flickrt.HashSetFromList"),
    (RUNTIME, "hashSetInsert", "flickrt.HashSetInsert"),
    (RUNTIME, "hashSetRemove", "flickrt.HashSetRemove"),
    (RUNTIME, "hashSetMember", "flickrt.HashSetMember"),
    (RUNTIME, "hashSetSize", "flickrt.HashSetSize"),
    (RUNTIME, "hashSetToList", "flickrt.HashSetToList"),
    (RUNTIME, "hashSetUnion", "flickrt.HashSetUnion"),
    (RUNTIME, "hashSetRehash", "flickrt.HashSetRehash"),
    (RUNTIME, "hashMapWith", "flickrt.HashMapWith"),
    (RUNTIME, "hashMapEmpty", "flickrt.HashMapEmpty"),
    (RUNTIME, "hashMapInsert", "flickrt.HashMapInsert"),
    (RUNTIME, "hashMapRemove", "flickrt.HashMapRemove"),
    (RUNTIME, "hashMapGet", "flickrt.HashMapGet"),
    (RUNTIME, "hashMapSize", "flickrt.HashMapSize"),
    (RUNTIME, "hashMapToList", "flickrt.HashMapToList"),
    (RUNTIME, "hashMapUnion", "flickrt.HashMapUnion"),
    (RUNTIME, "hashMapRehash", "flickrt.HashMapRehash"),
    (RUNTIME, "redactedMake", "flickrt.RedactedMake"),
    (RUNTIME, "redactedReveal", "flickrt.RedactedReveal"),
    (RUNTIME, "loop", "flickrt.Loop"),
    (RUNTIME, "validationMap2", "flickrt.ValidationMap2"),
    (HASH_MAP, "flickFromList", "flickrt.HashMapFromList"),
    (HASH_MAP, "flickIsEmpty", "flickrt.HashMapIsEmpty"),
    (HASH_MAP, "flickHas", "flickrt.HashMapHas"),
    (HASH_MAP, "flickGetHash", "flickrt.HashMapGetHash"),
    (HASH_MAP, "flickHasHash", "flickrt.HashMapHasHash"),
    (HASH_MAP, "flickHasBy", "flickrt.HashMapHasBy"),
    (HASH_MAP, "flickKeys", "flickrt.HashMapKeys"),
    (HASH_MAP, "flickValues", "flickrt.HashMapValues"),
    (HASH_MAP, "flickModifyAt", "flickrt.HashMapModifyAt"),
    (HASH_MAP, "flickModifyHash", "flickrt.HashMapModifyHash"),
    (HASH_MAP, "flickModify", "flickrt.HashMapModify"),
    (HASH_MAP, "flickRemoveMany", "flickrt.HashMapRemoveMany"),
    (HASH_MAP, "flickSetMany", "flickrt.HashMapSetMany"),
    (HASH_MAP, "flickMap", "flickrt.HashMapMap"),
    (HASH_MAP, "flickFlatMap", "flickrt.HashMapFlatMap"),
    (HASH_MAP, "flickReduce", "flickrt.HashMapReduce"),
    (HASH_MAP, "flickFilter", "flickrt.HashMapFilter"),
    (HASH_MAP, "flickCompact", "flickrt.HashMapCompact"),
    (HASH_MAP, "flickFilterMap", "flickrt.HashMapFilterMap"),
    (HASH_MAP, "flickFindFirst", "flickrt.HashMapFindFirst"),
    (HASH_MAP, "flickSome", "flickrt.HashMapSome"),
    (HASH_MAP, "flickEvery", "flickrt.HashMapEvery"),
    (HASH_SET, "flickIsEmpty", "flickrt.HashSetIsEmpty"),
    (HASH_SET, "flickIntersection", "flickrt.HashSetIntersection"),
    (HASH_SET, "flickDifference", "flickrt.HashSetDifference"),
    (HASH_SET, "flickIsSubset", "flickrt.HashSetIsSubset"),
    (HASH_SET, "flickMap", "flickrt.HashSetMap"),
    (HASH_SET, "flickFilter", "flickrt.HashSetFilter"),
    (HASH_SET, "flickSome", "flickrt.HashSetSome"),
    (HASH_SET, "flickEvery", "flickrt.HashSetEvery"),
    (HASH_SET, "flickReduce", "flickrt.HashSetReduce"),
    (
        "@flick-lang/runtime/effect/Redacted",
        "flickEquivalent",
        "flickrt.RedactedEquivalent",
    ),
    (RUNTIME, "seqOfList", "flickrt.SeqOfList"),
    (RUNTIME, "seqFrom", "flickrt.SeqFrom"),
    (RUNTIME, "seqRange", "flickrt.SeqRange"),
    (RUNTIME, "seqRangeBy", "flickrt.SeqRangeBy"),
    (RUNTIME, "seqRangeTo", "flickrt.SeqRangeTo"),
    (RUNTIME, "seqIterate", "flickrt.SeqIterate"),
    (RUNTIME, "seqRepeat", "flickrt.SeqRepeat"),
    (RUNTIME, "seqCycle", "flickrt.SeqCycle"),
    (RUNTIME, "seqUnfold", "flickrt.SeqUnfold"),
    (RUNTIME, "seqAppend", "flickrt.SeqAppend"),
    (RUNTIME, "seqMap2", "flickrt.SeqMap2"),
    (RUNTIME, "seqMap", "flickrt.SeqMap"),
    (RUNTIME, "seqFilter", "flickrt.SeqFilter"),
    (RUNTIME, "seqFilterMap", "flickrt.SeqFilterMap"),
    (RUNTIME, "seqFlatMap", "flickrt.SeqFlatMap"),
    (RUNTIME, "seqTake", "flickrt.SeqTake"),
    (RUNTIME, "seqTakeWhile", "flickrt.SeqTakeWhile"),
    (RUNTIME, "seqDrop", "flickrt.SeqDrop"),
    (RUNTIME, "seqDropWhile", "flickrt.SeqDropWhile"),
    (RUNTIME, "seqZip", "flickrt.SeqZip"),
    (RUNTIME, "seqZipWith", "flickrt.SeqZipWith"),
    (RUNTIME, "seqEnumerate", "flickrt.SeqEnumerate"),
    (RUNTIME, "seqScan", "flickrt.SeqScan"),
    (RUNTIME, "seqChunks", "flickrt.SeqChunks"),
    (RUNTIME, "seqWindows", "flickrt.SeqWindows"),
    (RUNTIME, "seqToList", "flickrt.SeqToList"),
    (RUNTIME, "seqFold", "flickrt.SeqFold"),
    (RUNTIME, "seqAny", "flickrt.SeqAny"),
    (RUNTIME, "seqAll", "flickrt.SeqAll"),
    (RUNTIME, "seqFind", "flickrt.SeqFind"),
    (RUNTIME, "seqFirst", "flickrt.SeqFirst"),
    (RUNTIME, "seqEachEffect", "flickrt.SeqEach"),
    (RUNTIME, "seqFoldEffect", "flickrt.SeqFold"),
    (RUNTIME, "equals", "flickrt.Equals"),
    (RUNTIME, "compare", "flickrt.Compare"),
    (RUNTIME, "hash", "flickrt.Hash"),
    (RUNTIME, "show", "flickrt.Show"),
    (RUNTIME, "identity", "flickrt.Identity"),
    (RUNTIME, "listEquals", "flickrt.ListEquals"),
    (RUNTIME, "listCompare", "flickrt.ListCompare"),
    (RUNTIME, "listHash", "flickrt.ListHash"),
    (RUNTIME, "listShow", "flickrt.ListShow"),
    (RUNTIME, "stdoutLine", "flickrt.StdoutLine"),
    (RUNTIME, "stderrLine", "flickrt.StderrLine"),
    (RUNTIME, "stdoutWrite", "flickrt.StdoutWrite"),
    (RUNTIME, "platformArgs", "flickrt.PlatformArgs"),
    (RUNTIME, "platformEnv", "flickrt.PlatformEnv"),
    (RUNTIME, "flickTodo", "flickrt.Todo"),
    (RUNTIME, "flickAssert", "flickrt.Assert"),
    (RUNTIME, "flickEcho", "flickrt.Echo"),
    (RUNTIME, "refMake", "flickrt.RefMake"),
    (RUNTIME, "refGet", "flickrt.RefGet"),
    (RUNTIME, "refSame", "flickrt.RefSame"),
    (RUNTIME, "refSet", "flickrt.RefSet"),
    (RUNTIME, "refModify", "flickrt.RefModify"),
    (RUNTIME, "stringFromNumber", "flickrt.StringFromNumber"),
    (RUNTIME, "intParse", "flickrt.IntParse"),
    (RUNTIME, "intFromFloat", "flickrt.IntFromFloat"),
    (RUNTIME, "intTruncate", "flickrt.IntTruncate"),
    (RUNTIME, "intAbs", "flickrt.IntAbs"),
    (RUNTIME, "intCheckedAdd", "flickrt.IntCheckedAdd"),
    (RUNTIME, "intCheckedSub", "flickrt.IntCheckedSub"),
    (RUNTIME, "intCheckedMul", "flickrt.IntCheckedMul"),
    (RUNTIME, "intCheckedDiv", "flickrt.IntCheckedDiv"),
    (RUNTIME, "intCheckedRem", "flickrt.IntCheckedRem"),
    (RUNTIME, "intUnsafeDiv", "flickrt.IntUnsafeDiv"),
    (RUNTIME, "intUnsafeRem", "flickrt.IntUnsafeRem"),
    (RUNTIME, "bitAnd", "flickrt.BitAnd"),
    (RUNTIME, "bitOr", "flickrt.BitOr"),
    (RUNTIME, "bitXor", "flickrt.BitXor"),
    (RUNTIME, "bitNot", "flickrt.BitNot"),
    (RUNTIME, "shiftLeft", "flickrt.ShiftLeft"),
    (RUNTIME, "shiftRight", "flickrt.ShiftRight"),
    (
        RUNTIME,
        "shiftRightUnsignedInt",
        "flickrt.ShiftRightUnsignedInt",
    ),
    (RUNTIME, "numberStrictEqual", "flickrt.NumberStrictEqual"),
    (RUNTIME, "numberSameValue", "flickrt.NumberSameValue"),
    (RUNTIME, "listMap", "flickrt.ListMap"),
    (RUNTIME, "listMapIndexed", "flickrt.ListMapIndexed"),
    (RUNTIME, "listEachEffect", "flickrt.ListEach"),
    (RUNTIME, "listFold", "flickrt.ListFold"),
    (RUNTIME, "listFoldRight", "flickrt.ListFoldRight"),
    (RUNTIME, "listFilter", "flickrt.ListFilter"),
    (RUNTIME, "listFilterMap", "flickrt.ListFilterMap"),
    (RUNTIME, "listFlatMap", "flickrt.ListFlatMap"),
    (RUNTIME, "listMap2", "flickrt.ListMap2"),
    (RUNTIME, "optionMap2", "flickrt.OptionMap2"),
    (RUNTIME, "resultMap2", "flickrt.ResultMap2"),
    (RUNTIME, "listFind", "flickrt.ListFind"),
    (RUNTIME, "listFindIndex", "flickrt.ListFindIndex"),
    (RUNTIME, "listAny", "flickrt.ListAny"),
    (RUNTIME, "listAll", "flickrt.ListAll"),
    (RUNTIME, "listPartition", "flickrt.ListPartition"),
    (RUNTIME, "listSortBy", "flickrt.ListSortBy"),
    (RUNTIME, "listLength", "flickrt.ListLength"),
    (RUNTIME, "listGet", "flickrt.ListGet"),
    (RUNTIME, "listSet", "flickrt.ListSet"),
    (RUNTIME, "listSingleton", "flickrt.ListSingleton"),
    (RUNTIME, "listRepeat", "flickrt.ListRepeat"),
    (RUNTIME, "listRange", "flickrt.ListRange"),
    (RUNTIME, "listInitialize", "flickrt.ListInitialize"),
    (RUNTIME, "listReverse", "flickrt.ListReverse"),
    (RUNTIME, "listConcat", "flickrt.ListConcatAll"),
    (RUNTIME, "listTake", "flickrt.ListTake"),
    (RUNTIME, "listDrop", "flickrt.ListDrop"),
    (RUNTIME, "listSlice", "flickrt.ListSlice"),
    (RUNTIME, "listZip", "flickrt.ListZip"),
    (RUNTIME, "listUnzip", "flickrt.ListUnzip"),
    (RUNTIME, "stringJoin", "flickrt.StringJoin"),
    (RUNTIME, "stringReplaceAll", "flickrt.StringReplaceAll"),
    (RUNTIME, "stringChars", "flickrt.Chars"),
    (RUNTIME, "stringToNumber", "flickrt.StringToNumber"),
    (RUNTIME, "stringLength", "flickrt.Length"),
    (RUNTIME, "stringTrim", "flickrt.Trim"),
    (STRING, "trim", "flickrt.Trim"),
    (STRING, "trimStart", "flickrt.TrimStart"),
    (STRING, "trimEnd", "flickrt.TrimEnd"),
    (STRING, "length", "flickrt.Length"),
    (STRING, "isNonEmpty", "flickrt.IsNonEmpty"),
    (STRING, "split", "flickrt.Split"),
    (STRING, "includes", "flickrt.Includes"),
    (STRING, "startsWith", "flickrt.StartsWith"),
    (STRING, "endsWith", "flickrt.EndsWith"),
    (STRING, "indexOf", "flickrt.IndexOf"),
    (STRING, "lastIndexOf", "flickrt.LastIndexOf"),
    (STRING, "toUpperCase", "flickrt.ToUpperCase"),
    (STRING, "toLowerCase", "flickrt.ToLowerCase"),
    (STRING, "capitalize", "flickrt.Capitalize"),
    (STRING, "uncapitalize", "flickrt.Uncapitalize"),
    (STRING, "slice", "flickrt.Slice"),
    (STRING, "substring", "flickrt.Substring"),
    (STRING, "takeLeft", "flickrt.TakeLeft"),
    (STRING, "takeRight", "flickrt.TakeRight"),
    (STRING, "at", "flickrt.StringAt"),
    (STRING, "charAt", "flickrt.CharAt"),
    (STRING, "charCodeAt", "flickrt.CharCodeAt"),
    (STRING, "codePointAt", "flickrt.CodePointAt"),
    (STRING, "padStart", "flickrt.PadStart"),
    (STRING, "padEnd", "flickrt.PadEnd"),
    (STRING, "repeat", "flickrt.Repeat"),
];

/// The Go value or function of each binding to a JS global, which may mean
/// different things for different types: by the binding's module and name.
const GLOBALS: &[(&str, &str, &str)] = &[
    ("Int", "to_float", "flickrt.IntToFloat"),
    ("Numeric", "bigint_of", "flickrt.BigIntOfInt"),
    ("Numeric", "float_of_bigint", "flickrt.FloatOfBigInt"),
    ("Float", "pi", "flickrt.Pi"),
    ("Float", "infinity", "flickrt.Inf"),
    ("Float", "nan", "flickrt.NaN"),
    ("Float", "is_nan", "flickrt.FloatIsNaN"),
    ("Float", "is_finite", "flickrt.FloatIsFinite"),
    ("Float", "is_integer", "flickrt.FloatIsInteger"),
    ("Float", "is_safe_integer", "flickrt.FloatIsSafeInteger"),
    ("Float", "abs", "flickrt.FloatAbs"),
    ("Float", "floor", "flickrt.FloatFloor"),
    ("Float", "ceil", "flickrt.FloatCeil"),
    ("Float", "round", "flickrt.FloatRound"),
    ("Float", "trunc", "flickrt.FloatTrunc"),
    ("Float", "sqrt", "flickrt.FloatSqrt"),
];

/// The Go function of the binding `name` of `module` to `export` of `from`.
fn lookup(module: &str, name: &str, from: Option<&str>, export: &str) -> Option<&'static str> {
    let (table, key) = match from {
        Some(from) => (EXPORTS, (from, export)),
        None => (GLOBALS, (module, name)),
    };
    table
        .iter()
        .find(|(a, b, _)| (*a, *b) == key)
        .map(|(_, _, go)| *go)
}

/// How many parameters the Go function of a binding takes after its
/// dictionaries: every stage, though the JS function takes `arity` and
/// returns a function of the rest, as Effect's data-last functions do.
pub(super) fn parameters(f: &Foreign) -> usize {
    if f.arity == 0 {
        f.info.stages_taken()
    } else {
        f.stages.len().max(f.arity)
    }
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
        let taken = parameters(f);
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return Some(format!("var {var} flickrt.Value = {target}"));
        }
        let heads: Vec<String> = (0..f.dicts)
            .map(|i| format!("d{i}"))
            .chain((0..taken).map(|i| format!("a{i}")))
            .collect();
        // a JS function of no arguments behind a `Unit` parameter
        let args = if f.arity == 0 {
            &heads[..f.dicts]
        } else {
            &heads[..]
        };
        Some(format!(
            "func {}({}) flickrt.Value {{\n\treturn {target}({})\n}}",
            names::func(self.module, &f.name),
            param_list(&heads),
            args.join(", ")
        ))
    }
}
