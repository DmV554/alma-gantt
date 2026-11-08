//! The baseline snapshot file, `.alma/baselines/<plan>/<id>.toml` (`docs/format.md` D14,
//! ADR 0013). Written once by `alma baseline save` and read by variance and S-curve reports.
//! Instants are `YYYY-MM-DDTHH:MM` strings and money is a decimal string, so the file is exact
//! and diffs well.

use std::str::FromStr;

use alma_core::model::{SnapTask, Snapshot};
use alma_core::time::{DAY, Instant, format_instant, start_of};
use jiff::civil::Date;
use rust_decimal::Decimal;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, value};

/// Version of the snapshot layout.
pub const SNAPSHOT_FORMAT: i64 = 0;

/// The snapshot as text.
pub fn to_text(s: &Snapshot) -> String {
    let mut doc = DocumentMut::new();
    doc.insert("format", value(SNAPSHOT_FORMAT));
    doc.insert("baseline", value(s.baseline.as_str()));
    doc.insert("start", value(format_instant(s.start)));
    doc.insert("finish", value(format_instant(s.finish)));
    doc.insert("bac", value(s.bac.normalize().to_string()));
    let mut pv = Array::new();
    for (d, amount) in &s.pv {
        let mut pair = Array::new();
        pair.push(d.to_string());
        pair.push(amount.normalize().to_string());
        pair.decor_mut().set_prefix("\n  ");
        pv.push(pair);
    }
    pv.set_trailing("\n");
    pv.set_trailing_comma(true);
    doc.insert("pv", value(pv));
    let mut tasks = ArrayOfTables::new();
    for t in &s.tasks {
        let mut row = Table::new();
        row.insert("id", value(t.id.as_str()));
        row.insert("start", value(format_instant(t.start)));
        row.insert("finish", value(format_instant(t.finish)));
        row.insert("cost", value(t.cost.normalize().to_string()));
        tasks.push(row);
    }
    doc.insert("task", Item::ArrayOfTables(tasks));
    let header = "# Baseline snapshot written by `alma baseline save`. Do not edit: variance and\n\
                  # planned value are read from here. Commit it with the plan.\n";
    format!("{header}{doc}")
}

/// Parses `YYYY-MM-DDTHH:MM`.
pub fn parse_instant(text: &str) -> Option<Instant> {
    let (date, time) = text.split_once('T')?;
    let date: Date = date.parse().ok()?;
    let (h, m) = time.split_once(':')?;
    let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    if !(0..=24).contains(&h) || !(0..60).contains(&m) || h * 60 + m > DAY {
        return None;
    }
    Some(start_of(date) + h * 60 + m)
}

/// Reads a snapshot. The error says what is wrong, for a usage error.
pub fn from_text(text: &str) -> Result<Snapshot, String> {
    let doc: DocumentMut = text.parse().map_err(|e| format!("not valid TOML: {e}"))?;
    let str_of = |t: &dyn toml_edit::TableLike, key: &str| -> Result<String, String> {
        t.get(key)
            .and_then(Item::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("missing or non-string `{key}`"))
    };
    let instant = |t: &dyn toml_edit::TableLike, key: &str| -> Result<Instant, String> {
        let s = str_of(t, key)?;
        parse_instant(&s).ok_or_else(|| format!("`{key}` = \"{s}\" is not YYYY-MM-DDTHH:MM"))
    };
    let money = |s: &str| Decimal::from_str(s).map_err(|_| format!("`{s}` is not a number"));
    let root = doc.as_table();
    if root.get("format").and_then(Item::as_integer) != Some(SNAPSHOT_FORMAT) {
        return Err(format!(
            "unsupported snapshot format (expected {SNAPSHOT_FORMAT})"
        ));
    }
    let mut pv = Vec::new();
    for pair in root
        .get("pv")
        .and_then(Item::as_array)
        .ok_or("missing `pv`")?
        .iter()
    {
        let pair = pair.as_array().ok_or("`pv` holds [date, amount] pairs")?;
        let (Some(d), Some(a)) = (
            pair.get(0).and_then(|v| v.as_str()),
            pair.get(1).and_then(|v| v.as_str()),
        ) else {
            return Err("`pv` holds [date, amount] pairs".into());
        };
        let d: Date = d.parse().map_err(|_| format!("`{d}` is not a date"))?;
        pv.push((d, money(a)?));
    }
    if pv.windows(2).any(|w| w[0].0 >= w[1].0) {
        return Err("`pv` dates must increase".into());
    }
    let mut tasks = Vec::new();
    if let Some(aot) = root.get("task").and_then(Item::as_array_of_tables) {
        for t in aot.iter() {
            tasks.push(SnapTask {
                id: str_of(t, "id")?,
                start: instant(t, "start")?,
                finish: instant(t, "finish")?,
                cost: money(&str_of(t, "cost")?)?,
            });
        }
    }
    Ok(Snapshot {
        baseline: str_of(root, "baseline")?,
        start: instant(root, "start")?,
        finish: instant(root, "finish")?,
        bac: money(&str_of(root, "bac")?)?,
        tasks,
        pv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_exact() {
        let d = |m: u8, day: i8| Date::new(2027, m as i8, day).unwrap();
        let s = Snapshot {
            baseline: "b1".into(),
            start: start_of(d(1, 4)) + 9 * 60,
            finish: start_of(d(3, 5)) + 18 * 60,
            bac: Decimal::from_str("4714.500000000001").unwrap(),
            tasks: vec![SnapTask {
                id: "a".into(),
                start: start_of(d(1, 4)) + 9 * 60,
                finish: start_of(d(1, 8)) + 18 * 60,
                cost: Decimal::from_str("12.3").unwrap(),
            }],
            pv: vec![
                (d(1, 4), Decimal::from_str("1.5").unwrap()),
                (d(1, 5), Decimal::from_str("3").unwrap()),
            ],
        };
        let text = to_text(&s);
        assert!(
            text.contains("pv = [\n  [\"2027-01-04\", \"1.5\"],"),
            "{text}"
        );
        assert_eq!(from_text(&text).unwrap(), s);
        assert_eq!(to_text(&from_text(&text).unwrap()), text);
        assert!(from_text("format = 9").is_err());
        assert_eq!(parse_instant("2027-01-04T24:00"), Some(start_of(d(1, 5))));
        assert_eq!(parse_instant("2027-01-04T25:00"), None);
    }
}
