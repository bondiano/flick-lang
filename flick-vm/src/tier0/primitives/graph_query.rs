//! Counted `Graph.Query` and DOT/Mermaid rendering primitives.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use flick_core::ir::Lit;

use super::graph_ops::{
    edge_at, expect_graph, expect_int, graph_defect, graph_err, index_list, missing_node_msg,
    neighbor_of, node_at, opt_inner,
};
use super::list_ops::slice_reps;
use super::schema_ops::js_string_value;
use super::{Data, Error, GraphData, Heap, Value, charge};

pub(super) fn get_node(heap: &mut Heap, idx_val: &Value, val: &Value) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let rep = {
        let g = expect_graph(heap, val)?;
        node_at(g, idx).map(|p| g.nodes[p].data.representation())
    };
    match rep {
        Some(r) => {
            let v = heap.dup(&r)?;
            heap.construct("Prelude.Option.Some", vec![v])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

pub(super) fn has_node(heap: &Heap, idx_val: &Value, val: &Value) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let g = expect_graph(heap, val)?;
    Ok(Value::Bool(node_at(g, idx).is_some()))
}

pub(super) fn get_edge(heap: &mut Heap, idx_val: &Value, val: &Value) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let found = {
        let g = expect_graph(heap, val)?;
        edge_at(g, idx).map(|p| {
            let e = &g.edges[p];
            (e.source, e.target, e.data.representation())
        })
    };
    match found {
        Some((source, target, rep)) => {
            let data = heap.dup(&rep)?;
            let rec = heap.record(vec![
                ("source".into(), Value::Int(source)),
                ("target".into(), Value::Int(target)),
                ("data".into(), data),
            ])?;
            heap.construct("Prelude.Option.Some", vec![rec])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

pub(super) fn has_edge(
    heap: &Heap,
    src_val: &Value,
    dst_val: &Value,
    val: &Value,
) -> Result<Value, Error> {
    let s = expect_int(src_val)?;
    let t = expect_int(dst_val)?;
    let g = expect_graph(heap, val)?;
    if let Some(eis) = g.adj.get(&s) {
        for &ei in eis {
            if let Some(q) = edge_at(g, ei)
                && neighbor_of(g, s, &g.edges[q]) == t
            {
                return Ok(Value::Bool(true));
            }
        }
    }
    Ok(Value::Bool(false))
}

pub(super) fn incident_edges(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let result = {
        let g = expect_graph(heap, val)?;
        if node_at(g, n).is_none() {
            return graph_err(heap, &missing_node_msg(n));
        }
        let out = g.adj.get(&n).map_or(&[][..], Vec::as_slice);
        let inc = g.radj.get(&n).map_or(&[][..], Vec::as_slice);
        charge(fuel, out.len() + inc.len())?;
        let mut res = Vec::new();
        if g.directed {
            let (mut a, mut b) = (0, 0);
            while a < out.len() && b < inc.len() {
                match out[a].cmp(&inc[b]) {
                    Ordering::Less => {
                        res.push(out[a]);
                        a += 1;
                    }
                    Ordering::Greater => {
                        res.push(inc[b]);
                        b += 1;
                    }
                    Ordering::Equal => {
                        res.push(out[a]);
                        a += 1;
                        b += 1;
                    }
                }
            }
            res.extend_from_slice(&out[a..]);
            res.extend_from_slice(&inc[b..]);
        } else {
            let mut prev = -1;
            for &e in out {
                if e != prev {
                    res.push(e);
                    prev = e;
                }
            }
        }
        res
    };
    let list = index_list(heap, &result)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

pub(super) fn outgoing_edges(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let list_vec = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return graph_err(heap, "Cannot get outgoing edges of undirected graph");
        }
        if node_at(g, n).is_none() {
            return graph_err(heap, &missing_node_msg(n));
        }
        let out = g.adj.get(&n).cloned().unwrap_or_default();
        charge(fuel, out.len())?;
        out
    };
    let list = index_list(heap, &list_vec)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

pub(super) fn incoming_edges(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let list_vec = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return graph_err(heap, "Cannot get incoming edges of undirected graph");
        }
        if node_at(g, n).is_none() {
            return graph_err(heap, &missing_node_msg(n));
        }
        let inc = g.radj.get(&n).cloned().unwrap_or_default();
        charge(fuel, inc.len())?;
        inc
    };
    let list = index_list(heap, &list_vec)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

pub(super) fn edges_between(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s = expect_int(src_val)?;
    let t = expect_int(dst_val)?;
    let result = {
        let g = expect_graph(heap, val)?;
        if node_at(g, s).is_none() {
            return graph_err(heap, &missing_node_msg(s));
        }
        if node_at(g, t).is_none() {
            return graph_err(heap, &missing_node_msg(t));
        }
        let adj = g.adj.get(&s).map_or(&[][..], Vec::as_slice);
        charge(fuel, adj.len())?;
        let mut res = Vec::new();
        let mut prev = -1;
        for &ei in adj {
            if ei == prev {
                continue;
            }
            prev = ei;
            if let Some(q) = edge_at(g, ei)
                && neighbor_of(g, s, &g.edges[q]) == t
            {
                res.push(ei);
            }
        }
        res
    };
    let list = index_list(heap, &result)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

pub(super) fn degree(heap: &mut Heap, idx_val: &Value, val: &Value) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let d = {
        let g = expect_graph(heap, val)?;
        if g.directed {
            return graph_err(heap, "Cannot get degree of directed graph");
        }
        if node_at(g, n).is_none() {
            return graph_err(heap, &missing_node_msg(n));
        }
        i32::try_from(g.adj.get(&n).map_or(0, Vec::len)).unwrap_or(i32::MAX)
    };
    heap.construct("Prelude.Result.Ok", vec![Value::Int(d)])
}

pub(super) fn out_degree(heap: &mut Heap, idx_val: &Value, val: &Value) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let d = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return graph_err(heap, "Cannot get outgoing edges of undirected graph");
        }
        if node_at(g, n).is_none() {
            return graph_err(heap, &missing_node_msg(n));
        }
        i32::try_from(g.adj.get(&n).map_or(0, Vec::len)).unwrap_or(i32::MAX)
    };
    heap.construct("Prelude.Result.Ok", vec![Value::Int(d)])
}

pub(super) fn in_degree(heap: &mut Heap, idx_val: &Value, val: &Value) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let d = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return graph_err(heap, "Cannot get incoming edges of undirected graph");
        }
        if node_at(g, n).is_none() {
            return graph_err(heap, &missing_node_msg(n));
        }
        i32::try_from(g.radj.get(&n).map_or(0, Vec::len)).unwrap_or(i32::MAX)
    };
    heap.construct("Prelude.Result.Ok", vec![Value::Int(d)])
}

fn unique_neighbors(g: &GraphData, node: i32, incoming: bool) -> Vec<i32> {
    let adj = if incoming { &g.radj } else { &g.adj };
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    if let Some(eis) = adj.get(&node) {
        for &ei in eis {
            if let Some(q) = edge_at(g, ei) {
                let e = &g.edges[q];
                let next = if (!g.directed && e.source != node) || (g.directed && incoming) {
                    e.source
                } else {
                    e.target
                };
                if seen.insert(next) {
                    out.push(next);
                }
            }
        }
    }
    out
}

pub(super) fn neighbors(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let out = {
        let g = expect_graph(heap, val)?;
        unique_neighbors(g, n, false)
    };
    charge(fuel, out.len())?;
    index_list(heap, &out)
}

pub(super) fn successors(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let out = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return Err(graph_defect("Cannot get successors of undirected graph"));
        }
        unique_neighbors(g, n, false)
    };
    charge(fuel, out.len())?;
    index_list(heap, &out)
}

pub(super) fn predecessors(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let out = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return Err(graph_defect("Cannot get predecessors of undirected graph"));
        }
        unique_neighbors(g, n, true)
    };
    charge(fuel, out.len())?;
    index_list(heap, &out)
}

pub(super) fn neighbors_directed(
    heap: &mut Heap,
    idx_val: &Value,
    dir_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let n = expect_int(idx_val)?;
    let dir = heap.string_lossy(dir_val)?;
    let out = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return Err(graph_defect(
                "Cannot get directed neighbors of undirected graph",
            ));
        }
        unique_neighbors(g, n, dir == "incoming")
    };
    charge(fuel, out.len())?;
    index_list(heap, &out)
}

pub(super) fn label_text_val(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let text = label_text(heap, val, fuel)?;
    heap.literal(&Lit::String(text))
}

fn label_text(heap: &Heap, val: &Value, fuel: &mut u64) -> Result<String, Error> {
    charge(fuel, 1)?;
    match val {
        Value::Unit => Ok("undefined".into()),
        Value::Object(tok) => match &heap.object(tok.id)?.data {
            Data::BigInt(s) => Ok(super::bigint_text(s)?),
            Data::Record(_) => Ok("[object Object]".into()),
            Data::Tagged { tag, .. } if tag != "$list" && tag != "$tuple" => {
                Ok("[object Object]".into())
            }
            Data::ListSlice { .. } | Data::Tagged { .. } => {
                let reps = slice_reps(heap, val)?;
                let mut parts = Vec::with_capacity(reps.len());
                for r in &reps {
                    parts.push(label_text(heap, r, fuel)?);
                }
                Ok(parts.join(","))
            }
            _ => js_string_value(heap, val, fuel),
        },
        _ => js_string_value(heap, val, fuel),
    }
}

fn graphviz_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str(r"\n");
            }
            '\n' => out.push_str(r"\n"),
            _ => out.push(c),
        }
    }
    out
}

pub(super) fn to_graph_viz_render(
    heap: &mut Heap,
    name_opt: &Value,
    node_labels_val: &Value,
    edge_labels_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let name = match opt_inner(heap, name_opt)? {
        Some(v) => heap.string_lossy(v)?,
        None => "G".into(),
    };
    let nl_reps = slice_reps(heap, node_labels_val)?;
    let el_reps = slice_reps(heap, edge_labels_val)?;
    let mut nl = Vec::with_capacity(nl_reps.len());
    for r in &nl_reps {
        nl.push(heap.string_lossy(r)?);
    }
    let mut el = Vec::with_capacity(el_reps.len());
    for r in &el_reps {
        el.push(heap.string_lossy(r)?);
    }
    let g = expect_graph(heap, val)?;
    charge(fuel, nl.len() + el.len())?;
    let (kind, op) = if g.directed {
        ("digraph", "->")
    } else {
        ("graph", "--")
    };
    let mut lines = vec![format!(r#"{kind} "{}" {{"#, graphviz_escape(&name))];
    for (n, lbl) in g.nodes.iter().zip(nl.iter()) {
        lines.push(format!(
            r#"  "{}" [label="{}"];"#,
            n.index,
            graphviz_escape(lbl)
        ));
    }
    for (e, lbl) in g.edges.iter().zip(el.iter()) {
        lines.push(format!(
            r#"  "{}" {op} "{}" [label="{}"];"#,
            e.source,
            e.target,
            graphviz_escape(lbl)
        ));
    }
    lines.push("}".into());
    heap.literal(&Lit::String(lines.join("\n")))
}

fn mermaid_escape(s: &str) -> String {
    let mut cur = s.to_owned();
    for (from, to) in [
        ("#", "#35;"),
        ("\"", "#quot;"),
        ("<", "#lt;"),
        (">", "#gt;"),
        ("&", "#amp;"),
        ("[", "#91;"),
        ("]", "#93;"),
        ("{", "#123;"),
        ("}", "#125;"),
        ("(", "#40;"),
        (")", "#41;"),
        ("|", "#124;"),
        ("\\", "#92;"),
    ] {
        cur = cur.replace(from, to);
    }
    cur.replace("\r\n", "<br/>").replace(['\r', '\n'], "<br/>")
}

pub(super) fn to_mermaid_render(
    heap: &mut Heap,
    meta_tup: &Value,
    labels_tup: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let diag_opt = opt_inner(heap, heap.field(meta_tup, 0)?)?
        .map(|v| heap.string_lossy(v))
        .transpose()?;
    let dir_opt = opt_inner(heap, heap.field(meta_tup, 1)?)?
        .map(|v| heap.string_lossy(v))
        .transpose()?;
    let nl_reps = slice_reps(heap, heap.field(labels_tup, 0)?)?;
    let el_reps = slice_reps(heap, heap.field(labels_tup, 1)?)?;
    let sh_reps = slice_reps(heap, heap.field(labels_tup, 2)?)?;
    let nl: Vec<String> = nl_reps
        .iter()
        .map(|r| heap.string_lossy(r))
        .collect::<Result<_, _>>()?;
    let el: Vec<String> = el_reps
        .iter()
        .map(|r| heap.string_lossy(r))
        .collect::<Result<_, _>>()?;
    let sh: Vec<String> = sh_reps
        .iter()
        .map(|r| heap.string_lossy(r))
        .collect::<Result<_, _>>()?;
    let g = expect_graph(heap, val)?;
    charge(fuel, nl.len() + el.len())?;
    let diagram = diag_opt.unwrap_or_else(|| if g.directed { "flowchart" } else { "graph" }.into());
    let direction = dir_opt.unwrap_or_else(|| "TD".into());
    let mut lines = vec![format!("{diagram} {direction}")];
    for ((n, lbl), shape) in g.nodes.iter().zip(nl.iter()).zip(sh.iter()) {
        let (l, r) = match shape.as_str() {
            "rounded" => ("(", ")"),
            "circle" => ("((", "))"),
            "diamond" => ("{", "}"),
            "hexagon" => ("{{", "}}"),
            "stadium" => ("([", "])"),
            "subroutine" => ("[[", "]]"),
            "cylindrical" => ("[(", ")]"),
            _ => ("[", "]"),
        };
        lines.push(format!(r#"  {}{l}"{}"{r}"#, n.index, mermaid_escape(lbl)));
    }
    let op = if diagram == "flowchart" { "-->" } else { "---" };
    for (e, lbl) in g.edges.iter().zip(el.iter()) {
        let esc = mermaid_escape(lbl);
        if esc.is_empty() {
            lines.push(format!("  {} {op} {}", e.source, e.target));
        } else {
            lines.push(format!(r#"  {} {op}|"{esc}"| {}"#, e.source, e.target));
        }
    }
    heap.literal(&Lit::String(lines.join("\n")))
}
