//! The key registry: every key the format knows, with its tier (`docs/format.md` §2).
//!
//! A test compares this registry with the tables in `docs/format.md`, so the two cannot drift.

/// Scope tier of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Used by the M0 engine or lint.
    A,
    /// Accepted and validated now, used by a later milestone.
    B,
    /// Reserved for the named milestone; rejected with `E-FMT-05`.
    R(&'static str),
}

/// One known key.
#[derive(Debug, Clone, Copy)]
pub struct KeyDef {
    /// Table path: `top`, `project`, `task`, `task.deps`, …
    pub table: &'static str,
    /// Key name.
    pub key: &'static str,
    /// Tier.
    pub tier: Tier,
}

use Tier::{A, B, R};

macro_rules! keys {
    ($($table:literal => [$($key:literal : $tier:expr),* $(,)?]),* $(,)?) => {
        &[$($(KeyDef { table: $table, key: $key, tier: $tier },)*)*]
    };
}

/// Every key, grouped by table. Order inside a table is the canonical key order.
pub const KEYS: &[KeyDef] = keys! {
    "top" => [
        "project": A, "settings": A, "rules": A, "calendar": A, "resource": A, "cadence": B,
        "task": A, "release": R("M4"), "reserve": A, "risk": A, "funding": A,
        "contract": R("M3"), "scenario": R("M2"), "cashflow_item": R("M2"),
        "baseline": A, "ucp": R("a later milestone"),
    ],
    "project" => [
        "format": A, "name": A, "start": A, "calendar": A, "deadline": A, "currency": A,
        "description": B, "status_date": R("M4"), "estimate_class": R("M2"), "fx": R("M2"),
    ],
    "settings" => ["schedule": A, "cost": R("M2")],
    "settings.schedule" => [
        "hours_per_day": A, "days_per_week": A, "late_anchor": A, "critical_definition": A,
        "critical_threshold": A, "near_critical": A, "lag_calendar": A, "phase": A,
        "reporting_period": B, "rolling_wave_horizon": B, "three_point": A,
        "leveling": A,
        "progress_logic": R("M4"), "agile_drives": R("M4"), "control_threshold": R("M4"),
    ],
    "rules" => ["severity": A, "params": A],
    "reserve" => [
        "id": A, "kind": A, "amount": A, "scope": A, "basis": A, "risks": A,
    ],
    "risk" => [
        "id": A, "name": A, "probability": A, "impact": A, "impact_time": A, "sign": A,
        "status": A, "owner": A, "affects": A, "correlation_group": A,
    ],
    "funding" => ["id": A, "date": A, "amount": A, "source": A],
    "baseline" => [
        "id": A, "name": A, "date": A, "active": A, "parent": A, "change_ref": A, "reason": A,
        "approved_by": A,
    ],
    "calendar" => [
        "id": A, "name": A, "workdays": A, "hours": A, "exceptions": A,
        "week": R("M2"), "resource": R("M2"),
    ],
    "calendar.exceptions" => ["name": A, "from": A, "to": A, "working": A, "hours": A],
    "task" => [
        "id": A, "parent": A, "name": A, "kind": A, "wbs_role": A, "approach": A, "class": A,
        "owner": A, "description": B, "deliverable": B, "acceptance": A, "excludes": B,
        "assumptions": B, "references": B, "sched_type": A, "duration": A, "effort": A,
        "estimate": A, "calendar": A, "constraint": A, "deadline": A, "spans": A, "reserve": A,
        "deps": A, "assign": A, "fixed_cost": A, "allow": A, "tags": B, "custom": B,
        "actual_start": R("M4"), "actual_finish": R("M4"), "remaining": R("M4"),
        "physical_pct": R("M4"), "progress_rule": R("M4"), "ev_method": R("M4"),
        "points": R("M4"), "iteration": R("M4"), "crash": R("M3"), "overlap": R("M3"),
        "risk_ref": R("M2"), "payment": R("M2"), "payment_ref": R("M2"),
        "requirement_refs": R("M2"), "contract": R("M3"), "account_code": R("M2"),
        "org_unit": R("M2"), "effort_type": R("M2"), "apportion_to": R("M2"),
        "location": R("M2"),
    ],
    "task.deps" => [
        "pred": A, "type": A, "lag": A, "nature": A, "scope": A, "rationale": A,
    ],
    "task.estimate" => [
        "o": A, "m": A, "p": A, "dist": A, "basis": A, "confidence": A,
    ],
    "task.constraint" => ["type": A, "date": A],
    "task.spans" => ["from": A, "to": A],
    "task.assign" => [
        "resource": A, "units": A, "work": R("M2"), "quantity": R("M2"), "profile": R("M2"),
    ],
    "task.fixed_cost" => ["amount": B, "accrue": B, "category": B],
    "resource" => [
        "id": A, "name": A, "kind": A, "rate": A, "max_units": A, "calendar": A,
        "parent": R("M2"), "category": R("M2"), "skills": R("M2"), "org_unit": R("M2"),
        "availability": R("M2"), "available_from": R("M2"), "available_to": R("M2"),
        "rates": R("M2"), "bill_rate": R("M2"),
    ],
    "resource.rate" => ["amount": B, "per": B],
    "cadence" => [
        "id": B, "name": B, "prefix": B, "start": B, "length": B, "count": B, "calendar": B,
    ],
};

/// Looks up a key.
pub fn lookup(table: &str, key: &str) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.table == table && k.key == key)
}

/// Keys of a table, canonical order.
pub fn keys_of(table: &str) -> impl Iterator<Item = &'static KeyDef> + '_ {
    KEYS.iter().filter(move |k| k.table == table)
}

/// The closest non-reserved key of `table` within edit distance 2, for "did you mean" hints.
pub fn suggest(table: &str, key: &str) -> Option<&'static str> {
    keys_of(table)
        .filter(|k| !matches!(k.tier, Tier::R(_)))
        .map(|k| (edit_distance(key, k.key), k.key))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_close_keys() {
        assert_eq!(suggest("task", "duraton"), Some("duration"));
        assert_eq!(suggest("task", "zzz"), None);
        assert_eq!(suggest("task.deps", "prd"), Some("pred"));
    }

    #[test]
    fn no_duplicate_keys() {
        for (i, a) in KEYS.iter().enumerate() {
            for b in &KEYS[i + 1..] {
                assert!(
                    !(a.table == b.table && a.key == b.key),
                    "{}.{}",
                    a.table,
                    a.key
                );
            }
        }
    }
}
