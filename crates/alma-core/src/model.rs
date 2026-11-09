//! The plan as the engine sees it (`docs/format.md` §5–§6). Built by `alma-format`, read by
//! `alma-sched`. Vectors keep source order, which breaks ties everywhere.

use std::collections::BTreeMap;

use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::diag::{Severity, Span};
use crate::duration::{Duration, WorkUnits};

/// Defines a string-valued enum with `as_str`, `parse` and `VALUES`.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every accepted spelling, in declaration order.
            pub const VALUES: &'static [&'static str] = &[$($text),+];

            /// The spelling used in plan files.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            /// Parses the spelling used in plan files.
            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

string_enum! {
    /// Scheduling shape of a task (`kind`).
    TaskKind {
        /// Ordinary activity.
        Task = "task",
        /// Zero-duration event.
        Milestone = "milestone",
        /// Level of effort: dates derived from the tasks it spans.
        Loe = "loe",
        /// Time reserve.
        Buffer = "buffer",
    }
}

string_enum! {
    /// Scope meaning of a WBS node (`wbs_role`).
    WbsRole {
        /// Deliverable-oriented element.
        Deliverable = "deliverable",
        /// Management control point for scope, cost and schedule.
        ControlAccount = "control_account",
        /// Lowest WBS level, where work is estimated and managed.
        WorkPackage = "work_package",
        /// Not yet decomposed future work.
        PlanningPackage = "planning_package",
    }
}

string_enum! {
    /// Delivery approach of a branch (`approach`).
    Approach {
        /// Full CPM logic.
        Predictive = "predictive",
        /// Iteration-based container.
        Iterative = "iterative",
        /// Flow-based container.
        Flow = "flow",
    }
}

string_enum! {
    /// Milestone class (`class`).
    MilestoneClass {
        /// Contract date.
        Contractual = "contractual",
        /// Internal checkpoint.
        Internal = "internal",
        /// Event owned by a third party.
        External = "external",
    }
}

string_enum! {
    /// Which variable a task keeps fixed (`sched_type`).
    SchedType {
        /// Duration is the input.
        FixedDuration = "fixed_duration",
        /// Effort is the input; duration = effort / Σ units.
        FixedEffort = "fixed_effort",
        /// Units are the input.
        FixedUnits = "fixed_units",
    }
}

string_enum! {
    /// Dependency type.
    DepType {
        /// Finish to start.
        Fs = "FS",
        /// Start to start.
        Ss = "SS",
        /// Finish to finish.
        Ff = "FF",
        /// Start to finish.
        Sf = "SF",
    }
}

string_enum! {
    /// Dependency nature.
    DepNature {
        /// Inherent in the work.
        Mandatory = "mandatory",
        /// Chosen; may be changed when fast-tracking.
        Discretionary = "discretionary",
    }
}

string_enum! {
    /// Dependency scope.
    DepScope {
        /// Between project activities.
        Internal = "internal",
        /// On something outside the project.
        External = "external",
    }
}

string_enum! {
    /// Constraint type.
    ConstraintKind {
        /// As soon as possible.
        Asap = "ASAP",
        /// As late as possible.
        Alap = "ALAP",
        /// Start no earlier than.
        Snet = "SNET",
        /// Start no later than.
        Snlt = "SNLT",
        /// Finish no earlier than.
        Fnet = "FNET",
        /// Finish no later than.
        Fnlt = "FNLT",
        /// Must start on.
        Mso = "MSO",
        /// Must finish on.
        Mfo = "MFO",
    }
}

string_enum! {
    /// Kind of time reserve on a buffer.
    ReserveKind {
        /// Inside the baseline, for identified risks.
        Contingency = "contingency",
        /// Outside the baseline.
        Management = "management",
    }
}

string_enum! {
    /// Three-point distribution.
    Dist {
        /// tE = (o + 4m + p) / 6.
        Beta = "beta",
        /// tE = (o + m + p) / 3.
        Triangular = "triangular",
    }
}

string_enum! {
    /// What the backward pass starts from.
    LateAnchor {
        /// The project early finish.
        EarlyFinish = "early_finish",
        /// The project deadline.
        Deadline = "deadline",
    }
}

string_enum! {
    /// How the critical set is defined.
    CriticalDefinition {
        /// Total float at or below the threshold.
        FloatThreshold = "float_threshold",
        /// Least-float path.
        LongestPath = "longest_path",
    }
}

string_enum! {
    /// Calendar that lags are counted on.
    LagCalendar {
        /// The project calendar.
        Project = "project",
        /// The predecessor's calendar.
        Predecessor = "predecessor",
        /// The successor's calendar.
        Successor = "successor",
    }
}

string_enum! {
    /// Plan maturity.
    Phase {
        /// Being built.
        Draft = "draft",
        /// Reviewed, not baselined.
        Preliminary = "preliminary",
        /// Baselined.
        Baselined = "baselined",
    }
}

string_enum! {
    /// How the scheduler resolves resource over-allocation (SPEC.md §7.4, ADR 0011).
    Leveling {
        /// Dates follow logic only; over-allocation is reported, not resolved.
        Off = "off",
        /// Delay tasks within their free float only: the finish date never moves.
        Smoothing = "smoothing",
        /// Delay tasks as far as needed; the finish date may move.
        Leveling = "leveling",
    }
}

string_enum! {
    /// Whether a resource's own calendar limits the tasks it works on (ADR 0017).
    ResourceCalendars {
        /// A task works only when its calendar and every assigned resource's calendar do.
        Intersect = "intersect",
        /// Tasks follow their own calendar; resource calendars only set capacity.
        Ignore = "ignore",
    }
}

string_enum! {
    /// How the remaining work of a task that started out of sequence is scheduled (ADR 0018).
    ProgressLogic {
        /// The remaining work still waits for its unfinished predecessors.
        RetainedLogic = "retained_logic",
        /// Once a task has started, its links no longer hold back its remaining work.
        ProgressOverride = "progress_override",
    }
}

string_enum! {
    /// Reporting period.
    ReportingPeriod {
        /// One week.
        Week = "week",
        /// One month.
        Month = "month",
    }
}

string_enum! {
    /// Resource kind.
    ResourceKind {
        /// People or roles.
        Work = "work",
        /// Machines.
        Equipment = "equipment",
        /// Consumables.
        Material = "material",
        /// Money lines.
        Cost = "cost",
    }
}

string_enum! {
    /// Unit a rate is quoted per.
    RatePer {
        /// Per hour.
        Hour = "hour",
        /// Per day.
        Day = "day",
        /// Per week.
        Week = "week",
        /// Per month.
        Month = "month",
        /// Per use.
        Use = "use",
    }
}

string_enum! {
    /// When a fixed cost accrues.
    Accrue {
        /// At the task start.
        Start = "start",
        /// Spread over the task.
        Prorated = "prorated",
        /// At the task finish.
        End = "end",
    }
}

/// A whole plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// `[project]`.
    pub project: Project,
    /// `[settings.schedule]`.
    pub settings: ScheduleSettings,
    /// `[rules]`.
    pub rules: RuleConfig,
    /// `[[calendar]]`, source order.
    pub calendars: Vec<Calendar>,
    /// `[[resource]]`, source order.
    pub resources: Vec<Resource>,
    /// `[[cadence]]`, source order.
    pub cadences: Vec<Cadence>,
    /// `[[task]]`, source (WBS) order.
    pub tasks: Vec<Task>,
    /// `[[baseline]]`, source order (ADR 0013).
    pub baselines: Vec<Baseline>,
    /// `[[reserve]]`, source order (ADR 0014).
    pub reserves: Vec<CostReserve>,
    /// `[[risk]]`, source order (ADR 0015).
    pub risks: Vec<Risk>,
    /// `[[funding]]`, source order (ADR 0015).
    pub funding: Vec<Funding>,
}

string_enum! {
    /// Whether a risk costs or saves.
    RiskSign {
        /// A threat: adds to the expected cost.
        Threat = "threat",
        /// An opportunity: subtracts.
        Opportunity = "opportunity",
    }
}

string_enum! {
    /// Life cycle of a risk.
    RiskStatus {
        /// Still possible.
        Open = "open",
        /// Occurred, avoided or expired.
        Closed = "closed",
    }
}

/// `[[risk]]`: an identified risk (knowledge notes, resources §7.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Risk {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Probability, 0 to 1.
    pub probability: Decimal,
    /// Cost impact if it occurs.
    pub impact: Option<Decimal>,
    /// Schedule impact if it occurs.
    pub impact_time: Option<Duration>,
    /// Threat or opportunity.
    pub sign: RiskSign,
    /// Open or closed.
    pub status: RiskStatus,
    /// Owner.
    pub owner: Option<String>,
    /// Tasks it affects.
    pub affects: Vec<String>,
    /// Risks with a common cause share a group.
    pub correlation_group: Option<String>,
}

/// `[[funding]]`: money that becomes available on a date (knowledge notes, resources §4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Funding {
    /// Id.
    pub id: String,
    /// Day the money is available.
    pub date: Date,
    /// Amount.
    pub amount: Decimal,
    /// Where it comes from.
    pub source: Option<String>,
}

/// `[[reserve]]`: a cost reserve. Contingency is part of the cost baseline; management reserve
/// is outside it but inside the project budget (knowledge notes, resources §4).
#[derive(Debug, Clone, PartialEq)]
pub struct CostReserve {
    /// Id.
    pub id: String,
    /// Contingency or management.
    pub kind: ReserveKind,
    /// Amount in project currency.
    pub amount: Decimal,
    /// Task it covers (its finish is when contingency accrues); `None` = the whole project.
    pub scope: Option<String>,
    /// Basis: how the amount was sized.
    pub basis: Option<String>,
    /// Risks it covers.
    pub risks: Vec<String>,
}

/// How progress on a task earns its budget (F-COST-14, ADR 0019).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvMethod {
    /// Physical percent complete, measured on the work (`physical_pct`).
    Physical,
    /// Fixed formula `S/F`: S % of the budget is earned at the actual start, the rest at the
    /// actual finish (`0/100`, `50/50`, `20/80`).
    Fixed {
        /// Percent earned at the start, 0 to 100.
        start: u8,
    },
}

impl EvMethod {
    /// Parses `physical` or `S/F` with S + F = 100.
    pub fn parse(text: &str) -> Option<Self> {
        if text == "physical" {
            return Some(EvMethod::Physical);
        }
        let (a, b) = text.split_once('/')?;
        let (a, b): (u8, u8) = (a.parse().ok()?, b.parse().ok()?);
        (u16::from(a) + u16::from(b) == 100).then_some(EvMethod::Fixed { start: a })
    }

    /// The spelling used in plan files.
    pub fn as_string(self) -> String {
        match self {
            EvMethod::Physical => "physical".into(),
            EvMethod::Fixed { start } => format!("{start}/{}", 100 - start),
        }
    }
}

/// One task in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapTask {
    /// Task id.
    pub id: String,
    /// Early start.
    pub start: crate::time::Instant,
    /// Early finish.
    pub finish: crate::time::Instant,
    /// Cost of the task and its descendants.
    pub cost: Decimal,
    /// The task's own budget (its assignments and fixed cost, no reserves, no descendants):
    /// what earned value measures it against (ADR 0019).
    pub budget: Decimal,
    /// Cumulative planned value of the own budget at the end of each day on which some of it
    /// accrues, date order.
    pub pv: Vec<(Date, Decimal)>,
}

impl SnapTask {
    /// Planned value of the task's own budget through the end of `date` (F-COST-13).
    pub fn pv_through(&self, date: Date) -> Decimal {
        cumulative_through(&self.pv, date)
    }
}

fn cumulative_through(curve: &[(Date, Decimal)], date: Date) -> Decimal {
    match curve.binary_search_by(|(d, _)| d.cmp(&date)) {
        Ok(i) => curve[i].1,
        Err(0) => Decimal::ZERO,
        Err(i) => curve[i - 1].1,
    }
}

/// What a baseline froze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Baseline id.
    pub baseline: String,
    /// Project start.
    pub start: crate::time::Instant,
    /// Project finish.
    pub finish: crate::time::Instant,
    /// Budget at completion: the total cost.
    pub bac: Decimal,
    /// Tasks in plan order.
    pub tasks: Vec<SnapTask>,
    /// Cumulative planned cost at the end of each day on which some cost accrued, in date
    /// order. Days without an entry keep the previous value.
    pub pv: Vec<(Date, Decimal)>,
    /// True when tasks carry their own budget and planned value (snapshot format 1); format 0
    /// snapshots cannot be used for earned value.
    pub per_task: bool,
    /// For a revision (`alma baseline revise`): the status date it was made at. Planned value
    /// through that day is the parent's; after it, the plan's at the time (ADR 0020).
    pub revised_at: Option<Date>,
}

impl Snapshot {
    /// Cumulative planned value at the end of `date` (F-COST-13).
    pub fn pv_through(&self, date: Date) -> Decimal {
        cumulative_through(&self.pv, date)
    }
}

/// `[[baseline]]`: metadata of a saved baseline. The frozen dates and costs live in a sidecar
/// file (`docs/format.md` D14, ADR 0013).
#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// The day it was saved.
    pub date: Option<Date>,
    /// The baseline reports compare against by default; at most one.
    pub active: bool,
    /// The baseline this one revises.
    pub parent: Option<String>,
    /// Change request that approved it.
    pub change_ref: Option<String>,
    /// Why it was saved or revised.
    pub reason: Option<String>,
    /// Who approved it.
    pub approved_by: Option<String>,
}

/// `[project]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// Project name.
    pub name: String,
    /// Project start.
    pub start: Date,
    /// Default calendar id; `None` means the built-in standard calendar.
    pub calendar: Option<String>,
    /// Project deadline.
    pub deadline: Option<Date>,
    /// Currency label.
    pub currency: Option<String>,
    /// Narrative.
    pub description: Option<String>,
    /// Data date: progress is reported through the end of this day (ADR 0018).
    pub status_date: Option<Date>,
}

/// `[settings.schedule]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleSettings {
    /// Day and week lengths.
    pub units: WorkUnits,
    /// Backward pass anchor.
    pub late_anchor: LateAnchor,
    /// Critical set definition.
    pub critical_definition: CriticalDefinition,
    /// Critical float threshold.
    pub critical_threshold: Duration,
    /// Near-critical float threshold.
    pub near_critical: Duration,
    /// Calendar for lags.
    pub lag_calendar: LagCalendar,
    /// Plan maturity.
    pub phase: Phase,
    /// Reporting period.
    pub reporting_period: Option<ReportingPeriod>,
    /// Rolling-wave horizon.
    pub rolling_wave_horizon: Option<Duration>,
    /// Three-point formula.
    pub three_point: Dist,
    /// Resource leveling mode.
    pub leveling: Leveling,
    /// How resource calendars combine with task calendars.
    pub resource_calendars: ResourceCalendars,
    /// How out-of-sequence progress is scheduled (ADR 0018).
    pub progress_logic: ProgressLogic,
}

impl Default for ScheduleSettings {
    fn default() -> Self {
        Self {
            units: WorkUnits::default(),
            late_anchor: LateAnchor::EarlyFinish,
            critical_definition: CriticalDefinition::FloatThreshold,
            critical_threshold: Duration::ZERO,
            near_critical: Duration::working(5 * 8 * 60),
            lag_calendar: LagCalendar::Project,
            phase: Phase::Draft,
            reporting_period: None,
            rolling_wave_horizon: None,
            three_point: Dist::Beta,
            leveling: Leveling::Off,
            resource_calendars: ResourceCalendars::Intersect,
            progress_logic: ProgressLogic::RetainedLogic,
        }
    }
}

/// A rule parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    /// A duration.
    Duration(Duration),
    /// A number.
    Number(Decimal),
    /// Anything else, as written.
    Text(String),
}

/// `[rules]`: severity overrides (`None` = off) and parameters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleConfig {
    /// Severity per lint code; `None` turns the rule off.
    pub severity: BTreeMap<String, Option<Severity>>,
    /// Rule parameters.
    pub params: BTreeMap<String, ParamValue>,
}

/// A working interval within a day, in minutes from midnight, `start < end`.
pub type Interval = (u16, u16);

/// `[[calendar]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Calendar {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Working weekdays, index 0 = Monday.
    pub workdays: [bool; 7],
    /// Working intervals of a normal working day.
    pub hours: Vec<Interval>,
    /// Exceptions, source order.
    pub exceptions: Vec<CalendarException>,
}

impl Calendar {
    /// The built-in calendar: Monday to Friday, 09:00–13:00 and 14:00–18:00.
    pub fn standard() -> Self {
        Self {
            id: "standard".into(),
            name: "Standard".into(),
            workdays: [true, true, true, true, true, false, false],
            hours: vec![(9 * 60, 13 * 60), (14 * 60, 18 * 60)],
            exceptions: Vec::new(),
        }
    }

    /// Working minutes in a normal working day.
    pub fn day_minutes(&self) -> i64 {
        self.hours.iter().map(|(a, b)| i64::from(b - a)).sum()
    }
}

/// One calendar exception (inclusive date range).
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarException {
    /// Name, shown by `explain`.
    pub name: String,
    /// First day.
    pub from: Date,
    /// Last day (inclusive).
    pub to: Date,
    /// True when the range is working time.
    pub working: bool,
    /// Working intervals for a working exception; `None` uses the calendar's hours.
    pub hours: Option<Vec<Interval>>,
}

/// `[[resource]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: ResourceKind,
    /// Rate.
    pub rate: Option<Rate>,
    /// Capacity.
    pub max_units: Decimal,
    /// Calendar id.
    pub calendar: Option<String>,
}

/// A rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    /// Money per unit.
    pub amount: Decimal,
    /// The unit.
    pub per: RatePer,
}

/// `[[cadence]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Cadence {
    /// Id.
    pub id: String,
    /// Band label.
    pub name: String,
    /// Prefix of generated iteration ids.
    pub prefix: String,
    /// First iteration start.
    pub start: Date,
    /// Iteration length.
    pub length: Duration,
    /// Number of iterations.
    pub count: u32,
    /// Calendar id.
    pub calendar: Option<String>,
}

impl Cadence {
    /// Generated iteration ids, `<prefix><nn>`.
    pub fn iteration_ids(&self) -> Vec<String> {
        let width = if self.count > 99 { 3 } else { 2 };
        (1..=self.count)
            .map(|n| format!("{}{:0width$}", self.prefix, n, width = width))
            .collect()
    }
}

/// `[[task]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    /// Id.
    pub id: String,
    /// Parent task id.
    pub parent: Option<String>,
    /// Name.
    pub name: String,
    /// Scheduling shape (explicit or default `task`).
    pub kind: TaskKind,
    /// True when `kind` was written in the file.
    pub kind_explicit: bool,
    /// WBS role.
    pub wbs_role: Option<WbsRole>,
    /// Approach written on this node (not inherited).
    pub approach: Option<Approach>,
    /// Milestone class.
    pub class: Option<MilestoneClass>,
    /// Accountable person.
    pub owner: Option<String>,
    /// Dictionary: description.
    pub description: Option<String>,
    /// Dictionary: deliverable.
    pub deliverable: Option<String>,
    /// Acceptance criterion.
    pub acceptance: Option<String>,
    /// Dictionary: exclusions.
    pub excludes: Option<String>,
    /// Assumptions.
    pub assumptions: Vec<String>,
    /// References.
    pub references: Vec<String>,
    /// Scheduling type.
    pub sched_type: SchedType,
    /// Duration.
    pub duration: Option<Duration>,
    /// Effort.
    pub effort: Option<Duration>,
    /// Three-point estimate.
    pub estimate: Option<Estimate>,
    /// Calendar override.
    pub calendar: Option<String>,
    /// Constraint.
    pub constraint: Option<Constraint>,
    /// Soft deadline.
    pub deadline: Option<Date>,
    /// LOE span `(from, to)`.
    pub spans: Option<(String, String)>,
    /// Buffer reserve kind.
    pub reserve: Option<ReserveKind>,
    /// Predecessors, source order.
    pub deps: Vec<Dep>,
    /// Assignments.
    pub assign: Vec<Assignment>,
    /// Fixed cost.
    pub fixed_cost: Option<FixedCost>,
    /// Progress: the day work started (ADR 0018).
    pub actual_start: Option<Date>,
    /// Progress: the day work finished.
    pub actual_finish: Option<Date>,
    /// Progress: working time left on a started task at the status date.
    pub remaining: Option<Duration>,
    /// Earned value: physical percent complete, 0 to 100 (ADR 0019).
    pub physical_pct: Option<Decimal>,
    /// Earned value: how progress earns the task's budget.
    pub ev_method: Option<EvMethod>,
    /// Actual cost through the status date, cumulative; on any WBS node.
    pub actual_cost: Option<Decimal>,
    /// Lint codes silenced on this node.
    pub allow: Vec<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Free-form fields, values kept as written.
    pub custom: BTreeMap<String, String>,
    /// Position of the `[[task]]` header.
    pub span: Option<Span>,
}

impl Task {
    /// A task with the given id and name and every other field empty.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            parent: None,
            name: name.into(),
            kind: TaskKind::Task,
            kind_explicit: false,
            wbs_role: None,
            approach: None,
            class: None,
            owner: None,
            description: None,
            deliverable: None,
            acceptance: None,
            excludes: None,
            assumptions: Vec::new(),
            references: Vec::new(),
            sched_type: SchedType::FixedDuration,
            duration: None,
            effort: None,
            estimate: None,
            calendar: None,
            constraint: None,
            deadline: None,
            spans: None,
            reserve: None,
            deps: Vec::new(),
            assign: Vec::new(),
            fixed_cost: None,
            actual_start: None,
            actual_finish: None,
            remaining: None,
            physical_pct: None,
            ev_method: None,
            actual_cost: None,
            allow: Vec::new(),
            tags: Vec::new(),
            custom: BTreeMap::new(),
            span: None,
        }
    }
}

impl Task {
    /// True when any progress is recorded on the task.
    pub fn has_progress(&self) -> bool {
        self.actual_start.is_some()
            || self.actual_finish.is_some()
            || self.remaining.is_some()
            || self.physical_pct.is_some()
    }
}

/// One predecessor link.
#[derive(Debug, Clone, PartialEq)]
pub struct Dep {
    /// Predecessor id.
    pub pred: String,
    /// Type.
    pub kind: DepType,
    /// Lag (negative = lead).
    pub lag: Duration,
    /// Nature.
    pub nature: DepNature,
    /// Scope.
    pub scope: DepScope,
    /// Why the link, lag or lead exists.
    pub rationale: Option<String>,
}

impl Dep {
    /// A finish-to-start link with no lag.
    pub fn fs(pred: impl Into<String>) -> Self {
        Self {
            pred: pred.into(),
            kind: DepType::Fs,
            lag: Duration::ZERO,
            nature: DepNature::Mandatory,
            scope: DepScope::Internal,
            rationale: None,
        }
    }
}

/// Three-point estimate.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    /// Optimistic.
    pub o: Duration,
    /// Most likely.
    pub m: Duration,
    /// Pessimistic.
    pub p: Duration,
    /// Distribution; `None` uses the plan setting.
    pub dist: Option<Dist>,
    /// Basis of estimate.
    pub basis: Option<String>,
    /// Confidence label.
    pub confidence: Option<String>,
}

/// A constraint.
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    /// Type.
    pub kind: ConstraintKind,
    /// Date; required for every type except ASAP and ALAP.
    pub date: Option<Date>,
}

/// One assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// Resource id.
    pub resource: String,
    /// Fraction of capacity.
    pub units: Decimal,
}

/// A fixed cost.
#[derive(Debug, Clone, PartialEq)]
pub struct FixedCost {
    /// Amount.
    pub amount: Decimal,
    /// Accrual timing.
    pub accrue: Accrue,
    /// Category.
    pub category: Option<String>,
}
