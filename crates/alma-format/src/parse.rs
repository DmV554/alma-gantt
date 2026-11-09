//! Reading a plan file into the model (`docs/format.md`). Collects every diagnostic instead of
//! stopping at the first one, so an agent can fix them all in one round.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::str::FromStr;

use alma_core::diag::{Diagnostic, Severity, Span};
use alma_core::duration::{Duration, WorkUnits, parse_duration};
use alma_core::model::*;
use alma_core::{is_valid_id, time};
use jiff::civil::Date;
use rust_decimal::Decimal;
use toml_edit::{Item, TableLike, Value};

use crate::keys::{self, Tier};

/// The result of reading a plan: the model when there are no errors, plus every diagnostic.
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The plan, `None` when any `E-*` error was found.
    pub plan: Option<Plan>,
    /// Diagnostics in file order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Format version this build reads.
pub const FORMAT_VERSION: i64 = 0;

/// Parses plan text.
pub fn parse(src: &str) -> Parsed {
    let mut cx = Cx::new(src);
    let doc = match toml_edit::Document::parse(src.to_string()) {
        Ok(doc) => doc,
        Err(e) => {
            let span = e.span().map(|r| cx.span(r.start));
            cx.diags.push(
                Diagnostic::error("E-FMT-01", format!("TOML syntax error: {}", e.message()))
                    .with_span(span),
            );
            return cx.finish(None);
        }
    };
    let plan = read_plan(&mut cx, doc.as_table());
    cx.finish(plan)
}

struct Cx {
    line_starts: Vec<usize>,
    src_len: usize,
    diags: Vec<Diagnostic>,
}

impl Cx {
    fn new(src: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Self {
            line_starts,
            src_len: src.len(),
            diags: Vec::new(),
        }
    }

    fn span(&self, offset: usize) -> Span {
        let offset = offset.min(self.src_len);
        let line = self.line_starts.partition_point(|&s| s <= offset);
        let col = offset - self.line_starts[line - 1] + 1;
        Span { line, col }
    }

    fn at(&self, range: Option<Range<usize>>) -> Option<Span> {
        range.map(|r| self.span(r.start))
    }

    fn err(&mut self, code: &'static str, msg: impl Into<String>, at: Option<Span>) -> Diag<'_> {
        self.diags.push(Diagnostic::error(code, msg).with_span(at));
        Diag(self.diags.last_mut().expect("just pushed"))
    }

    fn finish(mut self, plan: Option<Plan>) -> Parsed {
        self.diags.sort_by_key(|d| {
            let s = d.span.unwrap_or(Span { line: 0, col: 0 });
            (s.line, s.col)
        });
        let ok = !self.diags.iter().any(|d| d.severity == Severity::Error);
        Parsed {
            plan: if ok { plan } else { None },
            diagnostics: self.diags,
        }
    }
}

/// Builder handle to add ids or hints to the diagnostic just pushed.
struct Diag<'a>(&'a mut Diagnostic);

impl Diag<'_> {
    fn ids<S: Into<String>>(self, ids: impl IntoIterator<Item = S>) -> Self {
        self.0.ids.extend(ids.into_iter().map(Into::into));
        self
    }

    fn hint(self, hint: impl Into<String>) -> Self {
        self.0.hint = Some(hint.into());
        self
    }
}

fn item_span(cx: &Cx, item: &Item) -> Option<Span> {
    cx.at(item.span())
}

/// Checks the keys of a table against the registry: unknown → `E-FMT-02`, reserved →
/// `E-FMT-05`. `owner` names the object for messages.
fn check_keys(cx: &mut Cx, table: &str, t: &dyn TableLike, owner: &str) {
    let mut found: Vec<(String, Option<Span>)> = Vec::new();
    for (key, _) in t.iter() {
        let span = t.get_key_value(key).and_then(|(k, _)| cx.at(k.span()));
        found.push((key.to_string(), span));
    }
    for (key, span) in found {
        match keys::lookup(table, &key) {
            Some(def) => {
                if let Tier::R(milestone) = def.tier {
                    cx.err(
                        "E-FMT-05",
                        format!(
                            "`{key}` in {owner} is reserved for {milestone}, not supported yet"
                        ),
                        span,
                    )
                    .hint("remove the key; see docs/format.md for what this version accepts");
                }
            }
            None => {
                let msg = format!("unknown key `{key}` in {owner}");
                let d = cx.err("E-FMT-02", msg, span);
                match keys::suggest(table, &key) {
                    Some(s) => d.hint(format!("did you mean `{s}`?")),
                    None => d.hint("put free-form fields under `custom = { ... }`"),
                };
            }
        }
    }
}

/// Typed access to the fields of one table, reporting `E-FMT-03` on wrong types.
struct Fields<'a> {
    t: &'a dyn TableLike,
    /// Span of the whole table, for "missing key" errors.
    at: Option<Range<usize>>,
    owner: String,
    units: WorkUnits,
}

impl<'a> Fields<'a> {
    fn new(t: Tbl<'a>, owner: impl Into<String>, units: WorkUnits) -> Self {
        Self {
            t: t.0,
            at: t.1,
            owner: owner.into(),
            units,
        }
    }

    fn get(&self, key: &str) -> Option<&'a Item> {
        self.t.get(key).filter(|i| !i.is_none())
    }

    fn span(&self, cx: &Cx, key: &str) -> Option<Span> {
        self.get(key).and_then(|i| item_span(cx, i))
    }

    fn type_err(&self, cx: &mut Cx, key: &str, expected: &str) {
        let at = self.span(cx, key);
        cx.err(
            "E-FMT-03",
            format!("`{key}` in {} must be {expected}", self.owner),
            at,
        );
    }

    fn str(&self, cx: &mut Cx, key: &str) -> Option<String> {
        let item = self.get(key)?;
        match item.as_str() {
            Some(s) => Some(s.to_string()),
            None => {
                self.type_err(cx, key, "a string");
                None
            }
        }
    }

    fn required_str(&self, cx: &mut Cx, key: &str) -> Option<String> {
        if self.get(key).is_none() {
            let at = cx.at(self.at.clone());
            cx.err(
                "E-FMT-03",
                format!("{} is missing the required key `{key}`", self.owner),
                at,
            );
            return None;
        }
        self.str(cx, key)
    }

    fn bool(&self, cx: &mut Cx, key: &str) -> Option<bool> {
        let item = self.get(key)?;
        let v = item.as_bool();
        if v.is_none() {
            self.type_err(cx, key, "true or false");
        }
        v
    }

    fn date(&self, cx: &mut Cx, key: &str) -> Option<Date> {
        let item = self.get(key)?;
        let parsed = item.as_datetime().and_then(|dt| match (dt.date, dt.time) {
            (Some(d), None) => Date::new(d.year as i16, d.month as i8, d.day as i8).ok(),
            _ => None,
        });
        if parsed.is_none() {
            self.type_err(cx, key, "a date like 2027-01-04 (no time)");
        }
        parsed
    }

    fn int(&self, cx: &mut Cx, key: &str) -> Option<i64> {
        let item = self.get(key)?;
        let v = item.as_integer();
        if v.is_none() {
            self.type_err(cx, key, "an integer");
        }
        v
    }

    /// A number written as an integer or a float, converted exactly (at most 4 decimals).
    fn number(&self, cx: &mut Cx, key: &str) -> Option<Decimal> {
        let item = self.get(key)?;
        let v = item.as_value().and_then(value_number);
        if v.is_none() {
            self.type_err(cx, key, "a number with at most 4 decimals");
        }
        v
    }

    /// Money: a decimal string, never a TOML float.
    fn money(&self, cx: &mut Cx, key: &str) -> Option<Decimal> {
        let item = self.get(key)?;
        let v = item.as_str().and_then(|s| Decimal::from_str(s).ok());
        if v.is_none() {
            let at = self.span(cx, key);
            cx.err(
                "E-FMT-03",
                format!(
                    "`{key}` in {} must be a decimal string like \"4.2\"",
                    self.owner
                ),
                at,
            )
            .hint("money is written as a string so it never goes through a float");
        }
        v
    }

    fn duration(&self, cx: &mut Cx, key: &str, allow_negative: bool) -> Option<Duration> {
        let text = self.str(cx, key)?;
        match parse_duration(&text, self.units, allow_negative) {
            Ok(d) => Some(d),
            Err(e) => {
                let at = self.span(cx, key);
                cx.err("E-FMT-04", format!("{e} (`{key}` in {})", self.owner), at);
                None
            }
        }
    }

    fn strings(&self, cx: &mut Cx, key: &str) -> Vec<String> {
        let Some(item) = self.get(key) else {
            return Vec::new();
        };
        let arr = item.as_array();
        let out: Option<Vec<String>> =
            arr.and_then(|a| a.iter().map(|v| v.as_str().map(str::to_string)).collect());
        match out {
            Some(v) => v,
            None => {
                self.type_err(cx, key, "an array of strings");
                Vec::new()
            }
        }
    }

    fn enumerated<T>(
        &self,
        cx: &mut Cx,
        key: &str,
        parse: fn(&str) -> Option<T>,
        values: &[&str],
    ) -> Option<T> {
        let text = self.str(cx, key)?;
        let v = parse(&text);
        if v.is_none() {
            let at = self.span(cx, key);
            cx.err(
                "E-FMT-03",
                format!(
                    "`{key}` in {} is \"{text}\"; expected one of: {}",
                    self.owner,
                    values.join(", ")
                ),
                at,
            );
        }
        v
    }

    fn table(&self, cx: &mut Cx, key: &str) -> Option<Tbl<'a>> {
        let item = self.get(key)?;
        let t = item.as_table_like();
        if t.is_none() {
            self.type_err(cx, key, "a table");
        }
        t.map(|t| (t, item.span()))
    }
}

fn value_number(v: &Value) -> Option<Decimal> {
    match v {
        Value::Integer(i) => Some(Decimal::from(*i.value())),
        Value::Float(f) => {
            let raw = f
                .as_repr()
                .and_then(|r| r.as_raw().as_str())
                .map(str::to_string)
                .unwrap_or_else(|| f.value().to_string());
            let d = Decimal::from_str(raw.trim().replace('_', "").as_str()).ok()?;
            (d.scale() <= 4).then_some(d)
        }
        _ => None,
    }
}

/// A table and its span.
type Tbl<'a> = (&'a dyn TableLike, Option<Range<usize>>);

/// The table of an item, with its span.
fn tbl(item: &Item) -> Option<Tbl<'_>> {
    item.as_table_like().map(|t| (t, item.span()))
}

/// Iterates the inline or standard tables of an array (`deps = [{...}]` or `[[task]]`).
fn tables_of(item: &Item) -> Option<Vec<Tbl<'_>>> {
    if let Some(aot) = item.as_array_of_tables() {
        return Some(
            aot.iter()
                .map(|t| (t as &dyn TableLike, t.span()))
                .collect(),
        );
    }
    let arr = item.as_array()?;
    arr.iter()
        .map(|v| v.as_inline_table().map(|t| (t as &dyn TableLike, t.span())))
        .collect()
}

fn read_plan(cx: &mut Cx, root: &toml_edit::Table) -> Option<Plan> {
    check_keys(cx, "top", root, "the file");

    // [project]
    let Some(project_tbl) = root.get("project").and_then(tbl) else {
        cx.err(
            "E-FMT-03",
            "the file has no [project] table",
            Some(Span { line: 1, col: 1 }),
        )
        .hint("start the file with [project] format = 0, name and start");
        return None;
    };
    let project_t = project_tbl.0;
    check_keys(cx, "project", project_t, "[project]");
    let pf = Fields::new(project_tbl, "[project]", WorkUnits::default());
    match pf.get("format").map(|i| i.as_integer()) {
        Some(Some(FORMAT_VERSION)) => {}
        Some(other) => {
            let at = pf.span(cx, "format");
            cx.err(
                "E-FMT-06",
                format!(
                    "unsupported format version {}; this build reads version {FORMAT_VERSION}",
                    other.map_or("(not an integer)".to_string(), |v| v.to_string())
                ),
                at,
            );
        }
        None => {
            let at = cx.at(pf.at.clone());
            cx.err("E-FMT-06", "[project] has no `format` key", at)
                .hint(format!("add `format = {FORMAT_VERSION}`"));
        }
    }

    // [settings.schedule] first: durations everywhere depend on it.
    let settings = read_settings(cx, root);
    let units = settings.units;

    let name = pf.required_str(cx, "name");
    let start = if pf.get("start").is_none() {
        let at = cx.at(pf.at.clone());
        cx.err(
            "E-FMT-03",
            "[project] is missing the required key `start`",
            at,
        );
        None
    } else {
        pf.date(cx, "start")
    };
    let project = Project {
        name: name.unwrap_or_default(),
        start: start.unwrap_or(Date::constant(2000, 1, 1)),
        calendar: pf.str(cx, "calendar"),
        deadline: pf.date(cx, "deadline"),
        currency: pf.str(cx, "currency"),
        description: pf.str(cx, "description"),
    };

    let rules = read_rules(cx, root);
    let calendars = read_list(cx, root, "calendar", units, read_calendar);
    let resources = read_list(cx, root, "resource", units, read_resource);
    let cadences = read_list(cx, root, "cadence", units, read_cadence);
    let tasks = read_list(cx, root, "task", units, read_task);
    let risks = read_list(cx, root, "risk", units, read_risk);
    let reserves = read_list(cx, root, "reserve", units, read_reserve);
    let funding = read_list(cx, root, "funding", units, read_funding);
    let baselines = read_list(cx, root, "baseline", units, read_baseline);

    let plan = Plan {
        project,
        settings,
        rules,
        calendars,
        resources,
        cadences,
        tasks,
        baselines,
        reserves,
        risks,
        funding,
    };
    validate(cx, &plan, root);
    Some(plan)
}

type Reader<T> = fn(&mut Cx, &Fields<'_>) -> Option<T>;

fn read_list<T>(
    cx: &mut Cx,
    root: &toml_edit::Table,
    key: &str,
    units: WorkUnits,
    f: Reader<T>,
) -> Vec<T> {
    let Some(item) = root.get(key) else {
        return Vec::new();
    };
    let Some(tables) = item.as_array_of_tables() else {
        let at = item_span(cx, item);
        cx.err(
            "E-FMT-03",
            format!("`{key}` must be an array of tables: write each one as [[{key}]]"),
            at,
        );
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, t) in tables.iter().enumerate() {
        let owner = match t.get("id").and_then(Item::as_str) {
            Some(id) => format!("[[{key}]] `{id}`"),
            None => format!("[[{key}]] #{}", i + 1),
        };
        check_keys(cx, key, t, &owner);
        let fields = Fields::new((t, t.span()), owner, units);
        if let Some(v) = f(cx, &fields) {
            out.push(v);
        }
    }
    out
}

fn read_settings(cx: &mut Cx, root: &toml_edit::Table) -> ScheduleSettings {
    let mut s = ScheduleSettings::default();
    let Some(settings) = root.get("settings").and_then(Item::as_table_like) else {
        return s;
    };
    check_keys(cx, "settings", settings, "[settings]");
    let Some(st) = settings.get("schedule").and_then(tbl) else {
        return s;
    };
    check_keys(cx, "settings.schedule", st.0, "[settings.schedule]");
    let f = Fields::new(st.clone(), "[settings.schedule]", WorkUnits::default());
    if let Some(h) = f.number(cx, "hours_per_day") {
        let minutes = h * Decimal::from(60);
        if minutes.fract().is_zero() && minutes > Decimal::ZERO && minutes <= Decimal::from(1440) {
            s.units.minutes_per_day = minutes.try_into().unwrap_or(480);
        } else {
            f.type_err(cx, "hours_per_day", "between 0 and 24, in whole minutes");
        }
    }
    if let Some(d) = f.int(cx, "days_per_week") {
        if (1..=7).contains(&d) {
            s.units.days_per_week = d;
        } else {
            f.type_err(cx, "days_per_week", "between 1 and 7");
        }
    }
    let f = Fields::new(st, "[settings.schedule]", s.units);
    if let Some(v) = f.enumerated(cx, "late_anchor", LateAnchor::parse, LateAnchor::VALUES) {
        s.late_anchor = v;
    }
    if let Some(v) = f.enumerated(
        cx,
        "critical_definition",
        CriticalDefinition::parse,
        CriticalDefinition::VALUES,
    ) {
        s.critical_definition = v;
    }
    if let Some(v) = f.duration(cx, "critical_threshold", false) {
        s.critical_threshold = v;
    }
    if let Some(v) = f.duration(cx, "near_critical", false) {
        s.near_critical = v;
    }
    if let Some(v) = f.enumerated(cx, "lag_calendar", LagCalendar::parse, LagCalendar::VALUES) {
        s.lag_calendar = v;
    }
    if let Some(v) = f.enumerated(cx, "phase", Phase::parse, Phase::VALUES) {
        s.phase = v;
    }
    s.reporting_period = f.enumerated(
        cx,
        "reporting_period",
        ReportingPeriod::parse,
        ReportingPeriod::VALUES,
    );
    s.rolling_wave_horizon = f.duration(cx, "rolling_wave_horizon", false);
    if let Some(v) = f.enumerated(cx, "three_point", Dist::parse, Dist::VALUES) {
        s.three_point = v;
    }
    if let Some(v) = f.enumerated(cx, "leveling", Leveling::parse, Leveling::VALUES) {
        s.leveling = v;
    }
    s
}

fn is_lint_code(code: &str) -> bool {
    let mut parts = code.split('-');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some("L"), Some(area), Some(num), None)
            if !area.is_empty()
                && area.bytes().all(|b| b.is_ascii_uppercase())
                && num.len() == 2
                && num.bytes().all(|b| b.is_ascii_digit())
    )
}

fn read_rules(cx: &mut Cx, root: &toml_edit::Table) -> RuleConfig {
    let mut rules = RuleConfig::default();
    let Some(t) = root.get("rules").and_then(Item::as_table_like) else {
        return rules;
    };
    check_keys(cx, "rules", t, "[rules]");
    if let Some(sev) = t.get("severity").and_then(Item::as_table_like) {
        for (code, item) in sev.iter() {
            let at = sev.get_key_value(code).and_then(|(k, _)| cx.at(k.span()));
            if !is_lint_code(code) {
                let msg = if code.starts_with("E-") {
                    format!("`{code}` is a hard error and cannot be configured")
                } else {
                    format!("`{code}` is not a lint code like \"L-DEP-01\"")
                };
                cx.err("E-FMT-03", msg, at);
                continue;
            }
            let level = match item.as_str() {
                Some("off") => Some(None),
                Some("info") => Some(Some(Severity::Info)),
                Some("warn") => Some(Some(Severity::Warn)),
                Some("error") => Some(Some(Severity::Error)),
                _ => None,
            };
            match level {
                Some(l) => {
                    rules.severity.insert(code.to_string(), l);
                }
                None => {
                    cx.err(
                        "E-FMT-03",
                        format!("severity of `{code}` must be one of: off, info, warn, error"),
                        at,
                    );
                }
            }
        }
    }
    if let Some(params) = t.get("params").and_then(Item::as_table_like) {
        for (key, item) in params.iter() {
            let value = if let Some(s) = item.as_str() {
                match parse_duration(s, WorkUnits::default(), true) {
                    Ok(d) => ParamValue::Duration(d),
                    Err(_) => ParamValue::Text(s.to_string()),
                }
            } else if let Some(n) = item.as_value().and_then(value_number) {
                ParamValue::Number(n)
            } else {
                ParamValue::Text(item.to_string().trim().to_string())
            };
            rules.params.insert(key.to_string(), value);
        }
    }
    rules
}

fn parse_hhmm(s: &str) -> Option<u16> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 24 && m < 60 && h * 60 + m <= 1440).then_some(h * 60 + m)
}

/// Reads `hours` (a string or an array of `"HH:MM-HH:MM"`), reporting `E-CAL-02`.
fn read_hours(cx: &mut Cx, f: &Fields<'_>, key: &str) -> Option<Vec<Interval>> {
    let item = f.get(key)?;
    let texts: Option<Vec<&str>> = match item.as_str() {
        Some(s) => Some(vec![s]),
        None => item
            .as_array()
            .and_then(|a| a.iter().map(Value::as_str).collect()),
    };
    let at = item_span(cx, item);
    let Some(texts) = texts else {
        cx.err(
            "E-CAL-02",
            format!(
                "`{key}` in {} must be \"HH:MM-HH:MM\" or an array of them",
                f.owner
            ),
            at,
        );
        return None;
    };
    let mut out = Vec::new();
    for text in texts {
        let parsed = text
            .split_once('-')
            .and_then(|(a, b)| Some((parse_hhmm(a)?, parse_hhmm(b)?)))
            .filter(|(a, b)| a < b);
        match parsed {
            Some(iv) => out.push(iv),
            None => {
                cx.err(
                    "E-CAL-02",
                    format!("invalid working interval \"{text}\" in {}", f.owner),
                    at,
                )
                .hint("write intervals as \"09:00-13:00\", start before end, within one day");
                return None;
            }
        }
    }
    if out.windows(2).any(|w| w[0].1 > w[1].0) {
        cx.err(
            "E-CAL-02",
            format!("working intervals in {} overlap or are not sorted", f.owner),
            at,
        );
        return None;
    }
    Some(out)
}

const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

fn read_calendar(cx: &mut Cx, f: &Fields<'_>) -> Option<Calendar> {
    let id = f.required_str(cx, "id")?;
    let std = Calendar::standard();
    let mut workdays = std.workdays;
    if f.get("workdays").is_some() {
        workdays = [false; 7];
        for day in f.strings(cx, "workdays") {
            match WEEKDAYS.iter().position(|w| *w == day) {
                Some(i) => workdays[i] = true,
                None => {
                    let at = f.span(cx, "workdays");
                    cx.err(
                        "E-FMT-03",
                        format!("unknown weekday \"{day}\" in {}; use mon … sun", f.owner),
                        at,
                    );
                }
            }
        }
    }
    let hours = read_hours(cx, f, "hours").unwrap_or(std.hours);
    let mut exceptions = Vec::new();
    if let Some(item) = f.get("exceptions") {
        match tables_of(item) {
            Some(list) => {
                for (i, t) in list.into_iter().enumerate() {
                    let owner = format!("exception #{} of {}", i + 1, f.owner);
                    check_keys(cx, "calendar.exceptions", t.0, &owner);
                    let ef = Fields::new(t, owner, f.units);
                    let (Some(from), Some(to)) = (ef.date(cx, "from"), ef.date(cx, "to")) else {
                        let at = item_span(cx, item);
                        cx.err(
                            "E-FMT-03",
                            format!("{} needs `from` and `to` dates", ef.owner),
                            at,
                        );
                        continue;
                    };
                    if to < from {
                        let at = ef.span(cx, "to");
                        cx.err(
                            "E-FMT-03",
                            format!("{}: `to` is before `from`", ef.owner),
                            at,
                        );
                        continue;
                    }
                    exceptions.push(CalendarException {
                        name: ef.str(cx, "name").unwrap_or_default(),
                        from,
                        to,
                        working: ef.bool(cx, "working").unwrap_or(false),
                        hours: read_hours(cx, &ef, "hours"),
                    });
                }
            }
            None => f.type_err(cx, "exceptions", "an array of tables"),
        }
    }
    Some(Calendar {
        name: f.str(cx, "name").unwrap_or_else(|| id.clone()),
        id,
        workdays,
        hours,
        exceptions,
    })
}

fn read_resource(cx: &mut Cx, f: &Fields<'_>) -> Option<Resource> {
    let id = f.required_str(cx, "id")?;
    let rate = match f.table(cx, "rate") {
        Some(t) => {
            let owner = format!("`rate` of {}", f.owner);
            check_keys(cx, "resource.rate", t.0, &owner);
            let rf = Fields::new(t, owner, f.units);
            let amount = rf.money(cx, "amount");
            let per = rf.enumerated(cx, "per", RatePer::parse, RatePer::VALUES);
            match (amount, per) {
                (Some(amount), Some(per)) => Some(Rate { amount, per }),
                _ => {
                    if rf.get("amount").is_none() || rf.get("per").is_none() {
                        let at = f.span(cx, "rate");
                        cx.err(
                            "E-FMT-03",
                            format!("{} needs `amount` and `per`", rf.owner),
                            at,
                        );
                    }
                    None
                }
            }
        }
        None => None,
    };
    Some(Resource {
        name: f.str(cx, "name").unwrap_or_else(|| id.clone()),
        kind: f
            .enumerated(cx, "kind", ResourceKind::parse, ResourceKind::VALUES)
            .unwrap_or(ResourceKind::Work),
        rate,
        max_units: f.number(cx, "max_units").unwrap_or(Decimal::ONE),
        calendar: f.str(cx, "calendar"),
        id,
    })
}

fn read_cadence(cx: &mut Cx, f: &Fields<'_>) -> Option<Cadence> {
    let id = f.required_str(cx, "id")?;
    let start = f.date(cx, "start");
    let length = f.duration(cx, "length", false);
    let count = f.int(cx, "count");
    let (Some(start), Some(length), Some(count)) = (start, length, count) else {
        let at = cx.at(f.at.clone());
        cx.err(
            "E-FMT-03",
            format!("{} needs `start`, `length` and `count`", f.owner),
            at,
        );
        return None;
    };
    if !(1..=999).contains(&count) || length.minutes <= 0 {
        let at = f.span(cx, "count");
        cx.err(
            "E-FMT-03",
            format!(
                "{} needs `count` between 1 and 999 and a positive `length`",
                f.owner
            ),
            at,
        );
        return None;
    }
    Some(Cadence {
        name: f.str(cx, "name").unwrap_or_else(|| id.clone()),
        prefix: f.str(cx, "prefix").unwrap_or_else(|| id.clone()),
        start,
        length,
        count: count as u32,
        calendar: f.str(cx, "calendar"),
        id,
    })
}

fn read_reserve(cx: &mut Cx, f: &Fields<'_>) -> Option<CostReserve> {
    let id = f.required_str(cx, "id")?;
    if f.get("amount").is_none() {
        let at = cx.at(f.at.clone());
        cx.err(
            "E-FMT-03",
            format!("{} needs an `amount` like \"300\"", f.owner),
            at,
        );
        return None;
    }
    let amount = f.money(cx, "amount")?;
    Some(CostReserve {
        kind: f
            .enumerated(cx, "kind", ReserveKind::parse, ReserveKind::VALUES)
            .unwrap_or(ReserveKind::Contingency),
        amount,
        scope: f.str(cx, "scope"),
        basis: f.str(cx, "basis"),
        risks: f.strings(cx, "risks"),
        id,
    })
}

fn read_risk(cx: &mut Cx, f: &Fields<'_>) -> Option<Risk> {
    let id = f.required_str(cx, "id")?;
    let probability = f.number(cx, "probability");
    let Some(probability) = probability.filter(|p| *p >= Decimal::ZERO && *p <= Decimal::ONE)
    else {
        let at = f.span(cx, "probability").or_else(|| cx.at(f.at.clone()));
        cx.err(
            "E-FMT-03",
            format!(
                "{} needs a `probability` between 0 and 1, like 0.3",
                f.owner
            ),
            at,
        );
        return None;
    };
    Some(Risk {
        name: f.str(cx, "name").unwrap_or_else(|| id.clone()),
        probability,
        impact: f.money(cx, "impact"),
        impact_time: f.duration(cx, "impact_time", false),
        sign: f
            .enumerated(cx, "sign", RiskSign::parse, RiskSign::VALUES)
            .unwrap_or(RiskSign::Threat),
        status: f
            .enumerated(cx, "status", RiskStatus::parse, RiskStatus::VALUES)
            .unwrap_or(RiskStatus::Open),
        owner: f.str(cx, "owner"),
        affects: f.strings(cx, "affects"),
        correlation_group: f.str(cx, "correlation_group"),
        id,
    })
}

fn read_funding(cx: &mut Cx, f: &Fields<'_>) -> Option<Funding> {
    let id = f.required_str(cx, "id")?;
    let date = f.date(cx, "date");
    let amount = if f.get("amount").is_some() {
        f.money(cx, "amount")
    } else {
        None
    };
    let (Some(date), Some(amount)) = (date, amount) else {
        let at = cx.at(f.at.clone());
        cx.err(
            "E-FMT-03",
            format!("{} needs a `date` and an `amount` like \"1000\"", f.owner),
            at,
        );
        return None;
    };
    Some(Funding {
        date,
        amount,
        source: f.str(cx, "source"),
        id,
    })
}

fn read_baseline(cx: &mut Cx, f: &Fields<'_>) -> Option<Baseline> {
    let id = f.required_str(cx, "id")?;
    Some(Baseline {
        name: f.str(cx, "name").unwrap_or_else(|| id.clone()),
        date: f.date(cx, "date"),
        active: f.bool(cx, "active").unwrap_or(false),
        parent: f.str(cx, "parent"),
        change_ref: f.str(cx, "change_ref"),
        reason: f.str(cx, "reason"),
        approved_by: f.str(cx, "approved_by"),
        id,
    })
}

fn read_task(cx: &mut Cx, f: &Fields<'_>) -> Option<Task> {
    let id = f.required_str(cx, "id")?;
    let name = f.required_str(cx, "name").unwrap_or_default();
    let mut t = Task::new(id, name);
    t.span = cx.at(f.at.clone());
    t.parent = f.str(cx, "parent");
    if let Some(k) = f.enumerated(cx, "kind", TaskKind::parse, TaskKind::VALUES) {
        t.kind = k;
        t.kind_explicit = true;
    }
    t.wbs_role = f.enumerated(cx, "wbs_role", WbsRole::parse, WbsRole::VALUES);
    t.approach = f.enumerated(cx, "approach", Approach::parse, Approach::VALUES);
    t.class = f.enumerated(cx, "class", MilestoneClass::parse, MilestoneClass::VALUES);
    t.owner = f.str(cx, "owner");
    t.description = f.str(cx, "description");
    t.deliverable = f.str(cx, "deliverable");
    t.acceptance = f.str(cx, "acceptance");
    t.excludes = f.str(cx, "excludes");
    t.assumptions = f.strings(cx, "assumptions");
    t.references = f.strings(cx, "references");
    if let Some(s) = f.enumerated(cx, "sched_type", SchedType::parse, SchedType::VALUES) {
        t.sched_type = s;
    }
    t.duration = f.duration(cx, "duration", false);
    t.effort = f.duration(cx, "effort", false);
    if let Some(e) = f.table(cx, "estimate") {
        let owner = format!("`estimate` of {}", f.owner);
        check_keys(cx, "task.estimate", e.0, &owner);
        let ef = Fields::new(e, owner, f.units);
        let (o, m, p) = (
            ef.duration(cx, "o", false),
            ef.duration(cx, "m", false),
            ef.duration(cx, "p", false),
        );
        match (o, m, p) {
            (Some(o), Some(m), Some(p)) => {
                t.estimate = Some(Estimate {
                    o,
                    m,
                    p,
                    dist: ef.enumerated(cx, "dist", Dist::parse, Dist::VALUES),
                    basis: ef.str(cx, "basis"),
                    confidence: ef.str(cx, "confidence"),
                });
            }
            _ => {
                if ef.get("o").is_none() || ef.get("m").is_none() || ef.get("p").is_none() {
                    let at = f.span(cx, "estimate");
                    cx.err(
                        "E-FMT-03",
                        format!("{} needs `o`, `m` and `p`", ef.owner),
                        at,
                    );
                }
            }
        }
    }
    t.calendar = f.str(cx, "calendar");
    if let Some(c) = f.table(cx, "constraint") {
        let owner = format!("`constraint` of {}", f.owner);
        check_keys(cx, "task.constraint", c.0, &owner);
        let cf = Fields::new(c, owner, f.units);
        if let Some(kind) = cf.enumerated(cx, "type", ConstraintKind::parse, ConstraintKind::VALUES)
        {
            let date = cf.date(cx, "date");
            let needs_date = !matches!(kind, ConstraintKind::Asap | ConstraintKind::Alap);
            if needs_date && date.is_none() && cf.get("date").is_none() {
                let at = f.span(cx, "constraint");
                cx.err(
                    "E-FMT-03",
                    format!("{} of type {} needs a `date`", cf.owner, kind.as_str()),
                    at,
                );
            } else {
                t.constraint = Some(Constraint { kind, date });
            }
        }
    }
    t.deadline = f.date(cx, "deadline");
    if let Some(s) = f.table(cx, "spans") {
        let owner = format!("`spans` of {}", f.owner);
        check_keys(cx, "task.spans", s.0, &owner);
        let sf = Fields::new(s, owner, f.units);
        match (sf.str(cx, "from"), sf.str(cx, "to")) {
            (Some(a), Some(b)) => t.spans = Some((a, b)),
            _ => {
                let at = f.span(cx, "spans");
                cx.err(
                    "E-FMT-03",
                    format!("{} needs `from` and `to`", sf.owner),
                    at,
                );
            }
        }
    }
    t.reserve = f.enumerated(cx, "reserve", ReserveKind::parse, ReserveKind::VALUES);
    if let Some(item) = f.get("deps") {
        match tables_of(item) {
            Some(list) => {
                for (i, d) in list.into_iter().enumerate() {
                    let owner = format!("dependency #{} of {}", i + 1, f.owner);
                    check_keys(cx, "task.deps", d.0, &owner);
                    let df = Fields::new(d, owner, f.units);
                    let Some(pred) = df.required_str(cx, "pred") else {
                        continue;
                    };
                    let mut dep = Dep::fs(pred);
                    if let Some(k) = df.enumerated(cx, "type", DepType::parse, DepType::VALUES) {
                        dep.kind = k;
                    }
                    if let Some(l) = df.duration(cx, "lag", true) {
                        dep.lag = l;
                    }
                    if let Some(n) =
                        df.enumerated(cx, "nature", DepNature::parse, DepNature::VALUES)
                    {
                        dep.nature = n;
                    }
                    if let Some(s) = df.enumerated(cx, "scope", DepScope::parse, DepScope::VALUES) {
                        dep.scope = s;
                    }
                    dep.rationale = df.str(cx, "rationale");
                    t.deps.push(dep);
                }
            }
            None => f.type_err(cx, "deps", "an array of tables like [{ pred = \"a\" }]"),
        }
    }
    if let Some(item) = f.get("assign") {
        match tables_of(item) {
            Some(list) => {
                for (i, a) in list.into_iter().enumerate() {
                    let owner = format!("assignment #{} of {}", i + 1, f.owner);
                    check_keys(cx, "task.assign", a.0, &owner);
                    let af = Fields::new(a, owner, f.units);
                    if let Some(resource) = af.required_str(cx, "resource") {
                        t.assign.push(Assignment {
                            resource,
                            units: af.number(cx, "units").unwrap_or(Decimal::ONE),
                        });
                    }
                }
            }
            None => f.type_err(
                cx,
                "assign",
                "an array of tables like [{ resource = \"dev\" }]",
            ),
        }
    }
    if let Some(c) = f.table(cx, "fixed_cost") {
        let owner = format!("`fixed_cost` of {}", f.owner);
        check_keys(cx, "task.fixed_cost", c.0, &owner);
        let cf = Fields::new(c, owner, f.units);
        if let Some(amount) = cf.money(cx, "amount") {
            t.fixed_cost = Some(FixedCost {
                amount,
                accrue: cf
                    .enumerated(cx, "accrue", Accrue::parse, Accrue::VALUES)
                    .unwrap_or(Accrue::Prorated),
                category: cf.str(cx, "category"),
            });
        } else if cf.get("amount").is_none() {
            let at = f.span(cx, "fixed_cost");
            cx.err("E-FMT-03", format!("{} needs `amount`", cf.owner), at);
        }
    }
    t.allow = f.strings(cx, "allow");
    for code in &t.allow {
        if !is_lint_code(code) {
            let at = f.span(cx, "allow");
            cx.err(
                "E-FMT-03",
                format!(
                    "`allow` in {} lists \"{code}\", which is not a lint code",
                    f.owner
                ),
                at,
            );
        }
    }
    t.tags = f.strings(cx, "tags");
    if let Some((c, _)) = f.table(cx, "custom") {
        let mut custom = BTreeMap::new();
        for (k, v) in c.iter() {
            custom.insert(k.to_string(), v.to_string().trim().to_string());
        }
        t.custom = custom;
    }
    Some(t)
}

/// Cross-field and cross-object rules (`docs/format.md` §7).
fn validate(cx: &mut Cx, plan: &Plan, root: &toml_edit::Table) {
    let task_span = |i: usize| plan.tasks[i].span;

    // Ids: pattern and global uniqueness, in file order.
    let mut seen: HashMap<String, &'static str> = HashMap::new();
    let mut ids: Vec<(String, &'static str, Option<Span>)> = Vec::new();
    for c in &plan.calendars {
        ids.push((c.id.clone(), "calendar", None));
    }
    for r in &plan.resources {
        ids.push((r.id.clone(), "resource", None));
    }
    for c in &plan.cadences {
        ids.push((c.id.clone(), "cadence", None));
        for it in c.iteration_ids() {
            ids.push((it, "iteration", None));
        }
    }
    for t in &plan.tasks {
        ids.push((t.id.clone(), "task", t.span));
    }
    for b in &plan.baselines {
        ids.push((b.id.clone(), "baseline", None));
    }
    for r in &plan.reserves {
        ids.push((r.id.clone(), "reserve", None));
    }
    for r in &plan.risks {
        ids.push((r.id.clone(), "risk", None));
    }
    for r in &plan.funding {
        ids.push((r.id.clone(), "funding", None));
    }
    for (id, kind, span) in ids {
        if !is_valid_id(&id) {
            cx.err(
                "E-REF-02",
                format!("{kind} id \"{id}\" is not a valid id"),
                span,
            )
            .ids([id.clone()])
            .hint("ids use lowercase letters, digits and hyphens, start with a letter or digit, at most 63 characters");
        }
        if let Some(prev) = seen.get(&id) {
            cx.err(
                "E-REF-01",
                format!("duplicate id \"{id}\" ({kind}; already used by a {prev})"),
                span,
            )
            .ids([id.clone()]);
        } else {
            seen.insert(id, kind);
        }
    }

    let tasks: HashMap<&str, usize> = plan
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();
    let mut calendars: HashSet<&str> = plan.calendars.iter().map(|c| c.id.as_str()).collect();
    calendars.insert("standard");
    let resources: HashSet<&str> = plan.resources.iter().map(|r| r.id.as_str()).collect();

    let missing = |cx: &mut Cx, what: &str, id: &str, owner: &str, span: Option<Span>| {
        cx.err(
            "E-REF-03",
            format!("{owner} refers to unknown {what} \"{id}\""),
            span,
        )
        .ids([owner.trim_matches('`').to_string(), id.to_string()]);
    };

    if let Some(c) = &plan.project.calendar
        && !calendars.contains(c.as_str())
    {
        let span = root
            .get("project")
            .and_then(|p| p.get("calendar"))
            .and_then(|i| item_span(cx, i));
        missing(cx, "calendar", c, "[project]", span);
    }
    for r in &plan.resources {
        if let Some(c) = &r.calendar
            && !calendars.contains(c.as_str())
        {
            missing(cx, "calendar", c, &r.id, None);
        }
    }
    for c in &plan.cadences {
        if let Some(cal) = &c.calendar
            && !calendars.contains(cal.as_str())
        {
            missing(cx, "calendar", cal, &c.id, None);
        }
    }
    let risks: HashSet<&str> = plan.risks.iter().map(|r| r.id.as_str()).collect();
    for r in &plan.reserves {
        if let Some(scope) = &r.scope
            && !tasks.contains_key(scope.as_str())
        {
            missing(cx, "task", scope, &r.id, None);
        }
        for k in &r.risks {
            if !risks.contains(k.as_str()) {
                missing(cx, "risk", k, &r.id, None);
            }
        }
    }
    for r in &plan.risks {
        for t in &r.affects {
            if !tasks.contains_key(t.as_str()) {
                missing(cx, "task", t, &r.id, None);
            }
        }
    }
    let baselines: HashSet<&str> = plan.baselines.iter().map(|b| b.id.as_str()).collect();
    for b in &plan.baselines {
        if let Some(p) = &b.parent
            && (!baselines.contains(p.as_str()) || p == &b.id)
        {
            missing(cx, "baseline", p, &b.id, None);
        }
    }
    let active: Vec<String> = plan
        .baselines
        .iter()
        .filter(|b| b.active)
        .map(|b| b.id.clone())
        .collect();
    if active.len() > 1 {
        cx.err(
            "E-FMT-03",
            format!(
                "more than one baseline is active ({}); at most one can be",
                active.join(", ")
            ),
            None,
        )
        .ids(active.clone())
        .hint("keep `active = true` on one of them, or run `alma baseline activate <id>`");
    }

    let mut children = vec![0usize; plan.tasks.len()];
    for (i, t) in plan.tasks.iter().enumerate() {
        let span = task_span(i);
        if let Some(p) = &t.parent {
            match tasks.get(p.as_str()) {
                Some(&pi) => children[pi] += 1,
                None => missing(cx, "parent task", p, &t.id, span),
            }
        }
        if let Some(c) = &t.calendar
            && !calendars.contains(c.as_str())
        {
            missing(cx, "calendar", c, &t.id, span);
        }
        for d in &t.deps {
            if !tasks.contains_key(d.pred.as_str()) {
                missing(cx, "predecessor", &d.pred, &t.id, span);
            }
        }
        for a in &t.assign {
            if !resources.contains(a.resource.as_str()) {
                missing(cx, "resource", &a.resource, &t.id, span);
            }
        }
        if let Some((from, to)) = &t.spans {
            for x in [from, to] {
                if !tasks.contains_key(x.as_str()) {
                    missing(cx, "task in `spans`", x, &t.id, span);
                }
            }
        }
        // Key combinations.
        let combo = |cx: &mut Cx, msg: String| {
            cx.err("E-FMT-03", msg, span).ids([t.id.clone()]);
        };
        if t.spans.is_some() && t.kind != TaskKind::Loe {
            combo(
                cx,
                format!(
                    "`spans` is only valid with kind = \"loe\" (task `{}`)",
                    t.id
                ),
            );
        }
        if t.kind == TaskKind::Loe && t.spans.is_none() {
            combo(
                cx,
                format!(
                    "kind = \"loe\" needs `spans = {{ from, to }}` (task `{}`)",
                    t.id
                ),
            );
        }
        if t.reserve.is_some() && t.kind != TaskKind::Buffer {
            combo(
                cx,
                format!(
                    "`reserve` is only valid with kind = \"buffer\" (task `{}`)",
                    t.id
                ),
            );
        }
        if t.class.is_some() && t.kind != TaskKind::Milestone {
            combo(
                cx,
                format!(
                    "`class` is only valid with kind = \"milestone\" (task `{}`)",
                    t.id
                ),
            );
        }
        if t.sched_type == SchedType::FixedEffort && (t.effort.is_none() || t.assign.is_empty()) {
            combo(
                cx,
                format!(
                    "sched_type = \"fixed_effort\" needs `effort` and `assign` (task `{}`)",
                    t.id
                ),
            );
        }
    }
    for (i, t) in plan.tasks.iter().enumerate() {
        if children[i] > 0 && t.kind_explicit {
            cx.err(
                "E-FMT-03",
                format!(
                    "task `{}` has children, so it is a summary; remove `kind = \"{}\"`",
                    t.id,
                    t.kind.as_str()
                ),
                task_span(i),
            )
            .ids([t.id.clone()]);
        }
    }

    // Parent cycles.
    let mut state = vec![0u8; plan.tasks.len()];
    for start in 0..plan.tasks.len() {
        let mut path = Vec::new();
        let mut cur = Some(start);
        while let Some(i) = cur {
            if state[i] == 2 {
                break;
            }
            if state[i] == 1 {
                let pos = path.iter().position(|&x| x == i).unwrap_or(0);
                let ids: Vec<String> = path[pos..]
                    .iter()
                    .map(|&x: &usize| plan.tasks[x].id.clone())
                    .collect();
                cx.err(
                    "E-REF-04",
                    format!("`parent` cycle: {}", ids.join(" → ")),
                    task_span(i),
                )
                .ids(ids);
                break;
            }
            state[i] = 1;
            path.push(i);
            cur = plan.tasks[i]
                .parent
                .as_ref()
                .and_then(|p| tasks.get(p.as_str()).copied());
        }
        for i in path {
            state[i] = 2;
        }
    }

    // Calendars have working time.
    for c in &plan.calendars {
        let weekly: i64 = c.workdays.iter().filter(|&&w| w).count() as i64 * c.day_minutes();
        let extra = c.exceptions.iter().any(|e| e.working);
        if weekly == 0 && !extra {
            cx.err(
                "E-CAL-01",
                format!("calendar `{}` has no working time", c.id),
                None,
            )
            .ids([c.id.clone()]);
        }
    }

    // Exceptions must stay within a sane range (they are expanded day by day).
    for c in &plan.calendars {
        for e in &c.exceptions {
            if time::day_number(e.to) - time::day_number(e.from) > 366 * 100 {
                cx.err(
                    "E-FMT-03",
                    format!(
                        "exception \"{}\" of calendar `{}` spans more than 100 years",
                        e.name, c.id
                    ),
                    None,
                );
            }
        }
    }
}
