//! The canonical layout of a plan file (`docs/format.md` §3.1, ADR 0003): top-level items in a
//! fixed order, `[[task]]` tables in WBS order, keys in registry order, one blank line before
//! each table. Comments travel with the key or table they precede.
//!
//! `alma fmt` applies all of it; commands that add a new kind of table only call
//! [`place_tables`], so the rest of the file is left byte-identical.

use std::collections::HashMap;

use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

use crate::keys;

/// Top-level order.
const TOP: &[&str] = &[
    "project", "settings", "rules", "calendar", "resource", "cadence", "task",
];

/// Inline or sub-table key → registry table name.
const NESTED: &[(&str, &str, &str)] = &[
    ("calendar", "exceptions", "calendar.exceptions"),
    ("resource", "rate", "resource.rate"),
    ("task", "deps", "task.deps"),
    ("task", "estimate", "task.estimate"),
    ("task", "constraint", "task.constraint"),
    ("task", "spans", "task.spans"),
    ("task", "assign", "task.assign"),
    ("task", "fixed_cost", "task.fixed_cost"),
];

fn rank(registry: &str, key: &str) -> usize {
    keys::keys_of(registry)
        .position(|k| k.key == key)
        .unwrap_or(usize::MAX)
}

/// Gives every table a position in canonical top-level order, keeping the relative order of
/// tables of one kind. Rendering follows positions, so this only moves whole tables.
pub fn place_tables(doc: &mut DocumentMut) {
    let mut pos: isize = 0;
    let root = doc.as_table_mut();
    let mut names: Vec<String> = root.iter().map(|(k, _)| k.to_string()).collect();
    names.sort_by_key(|k| TOP.iter().position(|t| t == k).unwrap_or(usize::MAX));
    for name in names {
        if let Some(item) = root.get_mut(&name) {
            place_item(item, &mut pos);
        }
    }
}

fn place_item(item: &mut Item, pos: &mut isize) {
    match item {
        Item::Table(t) => place_table(t, pos),
        Item::ArrayOfTables(aot) => {
            for t in aot.iter_mut() {
                place_table(t, pos);
            }
        }
        _ => {}
    }
}

fn place_table(t: &mut Table, pos: &mut isize) {
    if !t.is_implicit() {
        t.set_position(Some(*pos));
        *pos += 1;
    }
    let subs: Vec<String> = t
        .iter()
        .filter(|(_, v)| v.is_table() || v.is_array_of_tables())
        .map(|(k, _)| k.to_string())
        .collect();
    for k in subs {
        if let Some(item) = t.get_mut(&k) {
            place_item(item, pos);
        }
    }
}

/// Reorders `[[task]]` tables into WBS order: each task right after its parent's subtree so
/// far, roots in file order. Tasks whose parent is unknown keep their place as roots.
fn wbs_order(doc: &mut DocumentMut) {
    let Some(aot) = doc.get_mut("task").and_then(Item::as_array_of_tables_mut) else {
        return;
    };
    let tables: Vec<Table> = aot.iter().cloned().collect();
    let ids: Vec<Option<String>> = tables
        .iter()
        .map(|t| t.get("id").and_then(Item::as_str).map(str::to_string))
        .collect();
    let by_id: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .filter_map(|(i, id)| id.as_deref().map(|id| (id, i)))
        .collect();
    let parent: Vec<Option<usize>> = tables
        .iter()
        .map(|t| {
            t.get("parent")
                .and_then(Item::as_str)
                .and_then(|p| by_id.get(p).copied())
        })
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); tables.len()];
    let mut roots = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match p {
            Some(p) if *p != i => children[*p].push(i),
            _ => roots.push(i),
        }
    }
    let mut order = Vec::with_capacity(tables.len());
    let mut seen = vec![false; tables.len()];
    let mut stack: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        order.push(i);
        stack.extend(children[i].iter().rev());
    }
    // Parent cycles are invalid files; keep those tables at the end rather than losing them.
    order.extend((0..tables.len()).filter(|&i| !seen[i]));
    if order.iter().enumerate().all(|(k, &i)| k == i) {
        return;
    }
    aot.clear();
    for i in order {
        aot.push(tables[i].clone());
    }
}

fn sort_table(t: &mut Table, registry: &str) {
    t.sort_values_by(|a, _, b, _| rank(registry, a.get()).cmp(&rank(registry, b.get())));
    for (outer, key, nested) in NESTED {
        if *outer != registry {
            continue;
        }
        match t.get_mut(key) {
            Some(Item::Value(Value::InlineTable(it))) => sort_inline(it, nested),
            Some(Item::Value(Value::Array(arr))) => {
                for v in arr.iter_mut() {
                    if let Value::InlineTable(it) = v {
                        sort_inline(it, nested);
                    }
                }
            }
            _ => {}
        }
    }
}

fn sort_inline(t: &mut InlineTable, registry: &str) {
    let before: Vec<String> = t.iter().map(|(k, _)| k.to_string()).collect();
    t.sort_values_by(|a, _, b, _| rank(registry, a.get()).cmp(&rank(registry, b.get())));
    let after: Vec<String> = t.iter().map(|(k, _)| k.to_string()).collect();
    if before != after {
        // Moving the last pair moves its trailing space too; reset the spacing.
        t.fmt();
    }
}

/// Puts one blank line before a table header, keeping any comments in front of it.
fn blank_line_before(t: &mut Table, first: bool) {
    let prefix = t
        .decor()
        .prefix()
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let body = prefix.trim_start_matches(['\n', '\r', ' ', '\t']);
    let want = if first {
        body.to_string()
    } else {
        format!("\n{body}")
    };
    if want != prefix {
        t.decor_mut().set_prefix(want);
    }
}

/// Applies the whole canonical layout.
pub fn canonicalize(doc: &mut DocumentMut) {
    wbs_order(doc);
    let root = doc.as_table_mut();
    let names: Vec<String> = root.iter().map(|(k, _)| k.to_string()).collect();
    for name in &names {
        match root.get_mut(name) {
            Some(Item::Table(t)) => {
                sort_table(t, name);
                // `[settings.schedule]` is the only sub-table with a key registry.
                if let Some(Item::Table(s)) = t.get_mut("schedule") {
                    sort_table(s, &format!("{name}.schedule"));
                }
            }
            Some(Item::ArrayOfTables(aot)) => {
                for t in aot.iter_mut() {
                    sort_table(t, name);
                }
            }
            _ => {}
        }
    }
    place_tables(doc);
    // Blank lines, in rendering (position) order.
    let mut tables: Vec<&mut Table> = Vec::new();
    collect_tables(doc.as_table_mut(), &mut tables);
    tables.sort_by_key(|t| t.position().unwrap_or(isize::MAX));
    for (i, t) in tables.into_iter().enumerate() {
        blank_line_before(t, i == 0);
    }
}

fn collect_tables<'a>(t: &'a mut Table, out: &mut Vec<&'a mut Table>) {
    for (_, item) in t.iter_mut() {
        match item {
            Item::Table(sub) => {
                if sub.is_implicit() {
                    collect_tables(sub, out);
                } else {
                    // Sub-tables of a real table are rare in plans ([settings.schedule] is
                    // under an implicit [settings]); treat the table itself only.
                    out.push(sub);
                }
            }
            Item::ArrayOfTables(aot) => out.extend(aot.iter_mut()),
            _ => {}
        }
    }
}

/// Formats plan text canonically. Fails only on TOML syntax errors.
pub fn format_text(src: &str) -> Result<String, toml_edit::TomlError> {
    let mut doc: DocumentMut = src.parse()?;
    canonicalize(&mut doc);
    let mut out = doc.to_string();
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}
