//! `ConfigProvider` unknown-tree and environment-trie node lookup primitives (Stage 4D).

use std::collections::HashMap;

use flick_core::ir::Lit;
use flick_core::opt::js_number;

use super::super::list_ops::slice_reps;
use super::super::{Data, Error, Fuel, Heap, Value, charge, show_value};

fn is_array_index(k: &str) -> bool {
    k.parse::<u32>()
        .ok()
        .is_some_and(|n| n < u32::MAX && n.to_string() == k)
}

fn js_keys(keys: &[String]) -> Vec<String> {
    let mut ints: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    for k in keys {
        if is_array_index(k) {
            ints.push(k.clone());
        } else {
            rest.push(k.clone());
        }
    }
    ints.sort_by_key(|k| k.parse::<u32>().unwrap_or(0));
    ints.extend(rest);
    ints
}

fn build_opt_str(heap: &mut Heap, val: Option<String>) -> Result<Value, Error> {
    match val {
        Some(s) => {
            let sv = heap.literal(&Lit::String(s))?;
            heap.construct("Prelude.Option.Some", vec![sv])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn build_value_node(heap: &mut Heap, v: String) -> Result<Value, Error> {
    let sv = heap.literal(&Lit::String(v))?;
    let node = heap.construct("ConfigProvider.Node.ValueNode", vec![sv])?;
    heap.construct("Prelude.Option.Some", vec![node])
}

fn build_record_node(
    heap: &mut Heap,
    keys: Vec<String>,
    val: Option<String>,
) -> Result<Value, Error> {
    let mut ks = Vec::with_capacity(keys.len());
    for k in keys {
        match heap.literal(&Lit::String(k)) {
            Ok(v) => ks.push(v),
            Err(err) => {
                heap.drop_values(ks)?;
                return Err(err);
            }
        }
    }
    let klist = heap.construct("$list", ks)?;
    let opt = match build_opt_str(heap, val) {
        Ok(o) => o,
        Err(err) => {
            heap.drop_value(klist)?;
            return Err(err);
        }
    };
    let node = heap.construct("ConfigProvider.Node.RecordNode", vec![klist, opt])?;
    heap.construct("Prelude.Option.Some", vec![node])
}

fn build_array_node(heap: &mut Heap, len: usize, val: Option<String>) -> Result<Value, Error> {
    let opt = build_opt_str(heap, val)?;
    let n = i32::try_from(len).unwrap_or(i32::MAX);
    let node = heap.construct("ConfigProvider.Node.ArrayNode", vec![Value::Int(n), opt])?;
    heap.construct("Prelude.Option.Some", vec![node])
}

fn segment_text(heap: &Heap, seg: &Value) -> Result<String, Error> {
    match seg {
        Value::Int(i) => Ok(i.to_string()),
        _ => heap.string_lossy(seg),
    }
}

fn string_from_any(heap: &Heap, val: &Value) -> Result<String, Error> {
    match val {
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(js_number(*f)),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::String(s) | Data::BigInt(s) => Ok(s.clone()),
            Data::Utf16(u) => Ok(String::from_utf16_lossy(u)),
            _ => show_value(heap, val),
        },
        _ => Ok(String::new()),
    }
}

pub(super) fn unknown_node_op(
    heap: &mut Heap,
    preserve_val: &Value,
    root: &Value,
    path_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let Value::Bool(preserve) = *preserve_val else {
        return Err(Error::ExpectedBoolean);
    };
    let path = slice_reps(heap, path_val)?;
    charge(fuel, path.len())?;
    let mut cur = root.representation();
    for seg in &path {
        let Value::Object(tok) = &cur else {
            return heap.construct("Prelude.Option.None", vec![]);
        };
        let next = match &heap.object(tok.id)?.data {
            Data::ListSlice { .. } => {
                let Value::Int(i) = *seg else {
                    return heap.construct("Prelude.Option.None", vec![]);
                };
                let elems = slice_reps(heap, &cur)?;
                if i < 0 || usize::try_from(i).unwrap_or(usize::MAX) >= elems.len() {
                    return heap.construct("Prelude.Option.None", vec![]);
                }
                elems[usize::try_from(i).unwrap_or(0)].representation()
            }
            Data::Tagged { tag, fields, .. } if tag == "$list" => {
                let Value::Int(i) = *seg else {
                    return heap.construct("Prelude.Option.None", vec![]);
                };
                if i < 0 || usize::try_from(i).unwrap_or(usize::MAX) >= fields.len() {
                    return heap.construct("Prelude.Option.None", vec![]);
                }
                fields[usize::try_from(i).unwrap_or(0)].representation()
            }
            Data::Record(entries) => {
                let Ok(k) = heap.string_lossy(seg) else {
                    return heap.construct("Prelude.Option.None", vec![]);
                };
                let Some((_, v)) = entries.iter().find(|(ek, _)| ek == &k) else {
                    return heap.construct("Prelude.Option.None", vec![]);
                };
                v.representation()
            }
            _ => return heap.construct("Prelude.Option.None", vec![]),
        };
        cur = next;
    }
    match &cur {
        Value::Unit => heap.construct("Prelude.Option.None", vec![]),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::JsonNull => heap.construct("Prelude.Option.None", vec![]),
            Data::String(s) => {
                let text = s.clone();
                if text.is_empty() && !preserve {
                    heap.construct("Prelude.Option.None", vec![])
                } else {
                    build_value_node(heap, text)
                }
            }
            Data::Utf16(u) => {
                let text = String::from_utf16_lossy(u);
                if text.is_empty() && !preserve {
                    heap.construct("Prelude.Option.None", vec![])
                } else {
                    build_value_node(heap, text)
                }
            }
            Data::ListSlice { .. } => {
                let len = slice_reps(heap, &cur)?.len();
                build_array_node(heap, len, None)
            }
            Data::Tagged { tag, fields, .. } if tag == "$list" => {
                let len = fields.len();
                build_array_node(heap, len, None)
            }
            Data::Record(entries) => {
                let keys: Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();
                build_record_node(heap, js_keys(&keys), None)
            }
            _ => {
                let text = string_from_any(heap, &cur)?;
                build_value_node(heap, text)
            }
        },
        _ => {
            let text = string_from_any(heap, &cur)?;
            build_value_node(heap, text)
        }
    }
}

#[derive(Default)]
struct EnvTrie {
    keys: Vec<String>,
    children: HashMap<String, EnvTrie>,
}

pub(super) fn env_node_op(
    heap: &mut Heap,
    preserve_val: &Value,
    pairs_val: &Value,
    path_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let Value::Bool(preserve) = *preserve_val else {
        return Err(Error::ExpectedBoolean);
    };
    let pair_list = slice_reps(heap, pairs_val)?;
    let path_list = slice_reps(heap, path_val)?;
    charge(fuel, pair_list.len() + path_list.len())?;

    let mut env = HashMap::new();
    let mut names = Vec::new();
    for p in &pair_list {
        let Value::Object(tok) = p else {
            return Err(Error::PatternKind("tuple"));
        };
        let Data::Tagged { fields, .. } = &heap.object(tok.id)?.data else {
            return Err(Error::PatternKind("tuple"));
        };
        let k = heap.string_lossy(&fields[0])?;
        let v = heap.string_lossy(&fields[1])?;
        if !env.contains_key(&k) {
            names.push(k.clone());
        }
        env.insert(k, v);
    }

    let mut root = EnvTrie::default();
    for name in &names {
        let mut node = &mut root;
        for seg in name.split('_') {
            if !node.children.contains_key(seg) {
                node.keys.push(seg.to_owned());
                node.children.insert(seg.to_owned(), EnvTrie::default());
            }
            node = node.children.get_mut(seg).expect("inserted");
        }
    }

    let mut parts = Vec::with_capacity(path_list.len());
    let mut node_opt = Some(&root);
    for seg in &path_list {
        let s = segment_text(heap, seg)?;
        if let Some(n) = node_opt {
            node_opt = n.children.get(&s);
        }
        parts.push(s);
    }

    let leaf = env.get(&parts.join("_")).and_then(|v| {
        if v.is_empty() && !preserve {
            None
        } else {
            Some(v.clone())
        }
    });

    let Some(node) = node_opt.filter(|n| !n.keys.is_empty()) else {
        return match leaf {
            Some(v) => build_value_node(heap, v),
            None => heap.construct("Prelude.Option.None", vec![]),
        };
    };

    let keys = js_keys(&node.keys);
    let mut max_idx: i64 = -1;
    for k in &keys {
        if !is_array_index(k) {
            return build_record_node(heap, keys, leaf);
        }
        let n = k.parse::<i64>().unwrap_or(0);
        if n > max_idx {
            max_idx = n;
        }
    }
    build_array_node(heap, usize::try_from(max_idx + 1).unwrap_or(0), leaf)
}
