//! Deterministic Markdown renderer for backend versioned value-ABI tables
//! (English and Russian reference and specification pages).

use std::fmt::Write as _;

use super::value_abi_spec::{ValueAbiEntrySpec, ValueAbiSpec};

/// Documentation locale for rendered value-ABI tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocLocale {
    En,
    Ru,
}

const JS_ABI_NAME: &str = "flick.javascript-ffi";

fn ru_notes<'a>(abi_name: &str, entry: &'a ValueAbiEntrySpec) -> &'a str {
    if abi_name != JS_ABI_NAME {
        return &entry.notes;
    }
    match entry.key.as_str() {
        "unit" => {
            "JS `undefined`; успешный внешний результат типа `Unit` — `()`, что бы ни вернул JS"
        }
        "bool" => "Примитив JS `boolean`",
        "int" => "Число JS `number`, хранимое в диапазоне знакового 32-битного целого (`Int32`)",
        "float" => "Число JS `number` (IEEE 754 binary64)",
        "bigint" => "JS `bigint` произвольной точности",
        "int8" => "Число JS `number`, приведённое к знаковому 8-битному целому",
        "int16" => "Число JS `number`, приведённое к знаковому 16-битному целому",
        "int64" => "JS `bigint`, обёрнутое по модулю знакового 64-битного целого",
        "uint8" => "Число JS `number`, приведённое к беззнаковому 8-битному целому",
        "uint16" => "Число JS `number`, приведённое к беззнаковому 16-битному целому",
        "uint32" => "Число JS `number`, приведённое к беззнаковому 32-битному целому",
        "uint64" => "JS `bigint`, обёрнутое по модулю беззнакового 64-битного целого",
        "float32" => "Число JS `number`, округлённое к IEEE 754 binary32",
        "string" => "Примитив JS `string` (UTF-16)",
        "never" => "Пустой тип без значений",
        "js_value" => "Непрозрачный `JsValue`, передаётся как есть до снимка с проверкой",
        "unknown" => "Инертное неизменяемое значение Flick; копируется на границе",
        "tuple" => "Плотный JS-массив фиксированной длины; копируется на границе",
        "list" => "Плотный JS-массив с неизменяемым API языка; копируется на границе",
        "option" => {
            "Нативный `Option` Effect; `nullable` читает `null`/`undefined` как `None`, а `optional i` передаёт `None` как `undefined`"
        }
        "result" => "Нативное представление `Result` закреплённой версии Effect",
        "eff" => "Нативное представление `Effect` закреплённой версии Effect",
        "layer" => "Нативное представление `Layer` закреплённой версии Effect",
        "exit" => "Нативное представление `Exit` закреплённой версии Effect",
        "failure" => "Нативное представление `Cause` закреплённой версии Effect",
        "func_ref" => {
            "Сама функция без захвата; адаптируется к арности JS, как функция обратного вызова"
        }
        "function" => {
            "Функции обратного вызова с адаптацией арности JS; четыре семейства: чистые, Effect, Promise и обработчики событий области"
        }
        _ => &entry.notes,
    }
}

fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// Renders a backend's [`ValueAbiSpec`] into a Markdown summary and table for
/// the requested `locale`.
pub fn render_value_abi_markdown(table: &ValueAbiSpec, locale: DocLocale) -> String {
    let mut out = String::new();
    match locale {
        DocLocale::En => {
            let catch_note = match &table.catch_lang_item {
                Some(item) => format!("`{}` (lang item `{item}`)", table.catch_error_type),
                None => format!("`{}`", table.catch_error_type),
            };
            let _ = writeln!(
                out,
                "**ABI:** `{}@{}` · **Opaque fallback:** `{}` · **Host `catch` type:** {catch_note}",
                table.abi.name, table.abi.version, table.opaque_type
            );
            out.push('\n');
            out.push_str("| Key | Flick | Host | Conversion | Direction | Notes |\n");
            out.push_str("| - | - | - | - | - | - |\n");
            for entry in &table.entries {
                let _ = writeln!(
                    out,
                    "| `{}` | `{}` | `{}` | `{}` | `{}` | {} |",
                    escape_cell(&entry.key),
                    escape_cell(&entry.flick_type),
                    escape_cell(&entry.host_type),
                    entry.conversion.as_str(),
                    entry.direction.as_str(),
                    escape_cell(&entry.notes),
                );
            }
            out.push('\n');
            if table.admits_copied_data {
                if table.abi.name == JS_ABI_NAME {
                    out.push_str("Records and user-defined ADTs have no stable layout and are not in this table. On this backend they may still cross bindings as deep-copied plain JS objects / tagged objects (`_tag` = `Name#hash`, fields `_0`, `_1`, …); `unboxed type` and `untagged type` declare explicit representations.\n");
                } else {
                    out.push_str("Records and user-defined ADTs have no stable layout and are not in this table; this backend admits them across bindings as deep-copied host values.\n");
                }
            } else {
                out.push_str("Records and user-defined ADTs have no stable layout and are not in this table; direct bindings refuse them unless wrapped in an opaque nominal handle.\n");
            }
        }
        DocLocale::Ru => {
            let catch_note = match &table.catch_lang_item {
                Some(item) => format!("`{}` (lang item `{item}`)", table.catch_error_type),
                None => format!("`{}`", table.catch_error_type),
            };
            let _ = writeln!(
                out,
                "**ABI:** `{}@{}` · **Непрозрачный вариант:** `{}` · **Тип хоста для `catch`:** {catch_note}",
                table.abi.name, table.abi.version, table.opaque_type
            );
            out.push('\n');
            out.push_str("| Ключ | Flick | Хост | Преобразование | Направление | Примечания |\n");
            out.push_str("| - | - | - | - | - | - |\n");
            for entry in &table.entries {
                let _ = writeln!(
                    out,
                    "| `{}` | `{}` | `{}` | `{}` | `{}` | {} |",
                    escape_cell(&entry.key),
                    escape_cell(&entry.flick_type),
                    escape_cell(&entry.host_type),
                    entry.conversion.as_str(),
                    entry.direction.as_str(),
                    escape_cell(ru_notes(&table.abi.name, entry)),
                );
            }
            out.push('\n');
            if table.admits_copied_data {
                if table.abi.name == JS_ABI_NAME {
                    out.push_str("Записи и пользовательские ADT не имеют стабильной раскладки и в эту таблицу не входят. В этом бэкенде они по-прежнему могут проходить через привязки с глубоким копированием как обычные JS-объекты / объекты с тегом (`_tag` = `Имя#hash`, поля `_0`, `_1`, …); `unboxed type` и `untagged type` объявляют явное представление.\n");
                } else {
                    out.push_str("Записи и пользовательские ADT не имеют стабильной раскладки и в эту таблицу не входят; этот бэкенд допускает их передачу через привязки с глубоким копированием значений.\n");
                }
            } else {
                out.push_str("Записи и пользовательские ADT не имеют стабильной раскладки и в эту таблицу не входят; прямые привязки отклоняют их, если не использовать непрозрачный номинальный дескриптор.\n");
            }
        }
    }
    out
}

/// Replaces the block delimited by `<!-- value-abi:{marker_id}:start -->` and
/// `<!-- value-abi:{marker_id}:end -->` inside `markdown` with `rendered`.
pub fn replace_value_abi_block(markdown: &str, marker_id: &str, rendered: &str) -> Option<String> {
    let start_marker = format!("<!-- value-abi:{marker_id}:start -->");
    let end_marker = format!("<!-- value-abi:{marker_id}:end -->");
    let start_idx = markdown.find(&start_marker)?;
    let body_start = start_idx + start_marker.len();
    let end_rel = markdown[body_start..].find(&end_marker)?;
    let end_idx = body_start + end_rel;
    let mut out = String::with_capacity(markdown.len() + rendered.len());
    out.push_str(&markdown[..body_start]);
    out.push('\n');
    out.push_str(rendered.trim_end());
    out.push('\n');
    out.push_str(&markdown[end_idx..]);
    Some(out)
}
