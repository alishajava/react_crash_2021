//! Task model and on-disk storage.
//!
//! Tasks are stored one per line as tab-separated values:
//! `id \t reminder(0|1) \t done(0|1) \t repeat \t day \t text`. Tabs, newlines
//! and backslashes inside fields are escaped so any text round-trips safely.
//! Older files with 4 fields (no `done`, no `repeat`) or 5 fields (no `repeat`)
//! are still accepted.
//!
//! `day` is free text, but the recommended form is `YYYY-MM-DD` or
//! `YYYY-MM-DD HH:MM`; days in that form are sorted chronologically and are the
//! only ones that can repeat.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// How often a task comes back after it is completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Never,
    Daily,
    Weekly,
    Monthly,
}

impl Repeat {
    /// The name used in the TSV file and in JSON (`""` for `Never`).
    pub fn as_str(self) -> &'static str {
        match self {
            Repeat::Never => "",
            Repeat::Daily => "daily",
            Repeat::Weekly => "weekly",
            Repeat::Monthly => "monthly",
        }
    }

    /// Accepts `""`/`none`/`never`, `daily`, `weekly`, `monthly`.
    pub fn parse(s: &str) -> Option<Repeat> {
        match s {
            "" | "none" | "never" => Some(Repeat::Never),
            "daily" => Some(Repeat::Daily),
            "weekly" => Some(Repeat::Weekly),
            "monthly" => Some(Repeat::Monthly),
            _ => None,
        }
    }
}

impl fmt::Display for Repeat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: u32,
    pub text: String,
    pub day: String,
    pub reminder: bool,
    pub done: bool,
    pub repeat: Repeat,
}

impl Task {
    /// The day as a sortable key, if it is in `YYYY-MM-DD[ HH:MM]` form.
    pub fn date_key(&self) -> Option<&str> {
        is_iso_day(&self.day).then_some(self.day.as_str())
    }

    /// Case-insensitive search: every whitespace-separated word of `query`
    /// must appear in the text or the day. An empty query matches everything.
    pub fn matches(&self, query: &str) -> bool {
        let text = self.text.to_lowercase();
        let day = self.day.to_lowercase();
        query.split_whitespace().all(|word| {
            let word = word.to_lowercase();
            text.contains(&word) || day.contains(&word)
        })
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bell = if self.reminder { "🔔" } else { "  " };
        let check = if self.done { "[x]" } else { "[ ]" };
        write!(f, "{bell} {check} {:>3}. {}", self.id, self.text)?;
        if !self.day.is_empty() {
            write!(f, "  — {}", self.day)?;
        }
        if self.repeat != Repeat::Never {
            write!(f, " 🔁{}", self.repeat)?;
        }
        Ok(())
    }
}

// ---------- dates ----------

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = (m as i64 + 9) % 12; // March = 0
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// `YYYY-MM-DD` (UTC) for a Unix timestamp in seconds.
pub fn iso_date_from_unix_secs(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM-DD` or `YYYY-MM-DD HH:MM` with a real calendar date and a valid time.
pub fn is_iso_day(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(99);
    let date_ok = b.len() >= 10
        && digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && (1..=12).contains(&num(5..7))
        && (1..=days_in_month(i64::from(num(0..4)), num(5..7))).contains(&num(8..10));
    match b.len() {
        10 => date_ok,
        16 => {
            date_ok
                && b[10] == b' '
                && digits(11..13)
                && b[13] == b':'
                && digits(14..16)
                && num(11..13) < 24
                && num(14..16) < 60
        }
        _ => false,
    }
}

const MAX_STEPS: u32 = 100_000;

/// The day a repeating task moves to once it is completed.
///
/// `day` must be in `YYYY-MM-DD[ HH:MM]` form (the time part is kept). The day
/// advances one step (1 day, 7 days or 1 month), then keeps advancing until it
/// is after `today` (`YYYY-MM-DD`), so an overdue task does not come back
/// already overdue. Months keep the original day-of-month where possible and
/// otherwise use the last day (Jan 31 → Feb 28 → Mar 31 when catching up).
/// Returns `None` if the task does not repeat or `day` is not in that form.
pub fn next_occurrence(day: &str, repeat: Repeat, today: &str) -> Option<String> {
    if repeat == Repeat::Never || !is_iso_day(day) {
        return None;
    }
    let anchor_d: u32 = day[8..10].parse().ok()?;
    let mut cur = (
        day[0..4].parse::<i64>().ok()?,
        day[5..7].parse::<u32>().ok()?,
        anchor_d,
    );
    let suffix = &day[10..];
    let today_ok = today.len() == 10 && is_iso_day(today);
    for _ in 0..MAX_STEPS {
        cur = match repeat {
            Repeat::Never => return None,
            Repeat::Daily | Repeat::Weekly => {
                let n = if repeat == Repeat::Daily { 1 } else { 7 };
                civil_from_days(days_from_civil(cur.0, cur.1, cur.2) + n)
            }
            Repeat::Monthly => {
                let (y, m) = if cur.1 == 12 {
                    (cur.0 + 1, 1)
                } else {
                    (cur.0, cur.1 + 1)
                };
                (y, m, anchor_d.min(days_in_month(y, m)))
            }
        };
        let date = format!("{:04}-{:02}-{:02}", cur.0, cur.1, cur.2);
        if !today_ok || date.as_str() > today {
            return Some(format!("{date}{suffix}"));
        }
    }
    None
}

// ---------- errors ----------

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Parse { line: usize, msg: String },
    NotFound(u32),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "I/O error: {e}"),
            StoreError::Parse { line, msg } => write!(f, "corrupt data on line {line}: {msg}"),
            StoreError::NotFound(id) => write!(f, "no task with id {id}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

// ---------- the list ----------

/// An in-memory list of tasks. Pure logic, no I/O — shared by the CLI and the web build.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TaskList {
    tasks: Vec<Task>,
}

impl TaskList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse the TSV storage format.
    pub fn from_tsv(s: &str) -> Result<Self, StoreError> {
        Ok(TaskList { tasks: parse(s)? })
    }

    pub fn to_tsv(&self) -> String {
        serialize(&self.tasks)
    }

    /// Tasks in the order they were added.
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn get(&self, id: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Tasks sorted for display: unfinished first; within each group, tasks with
    /// a `YYYY-MM-DD[ HH:MM]` day in date order, then the rest in the order added.
    pub fn sorted(&self) -> Vec<&Task> {
        let mut v: Vec<&Task> = self.tasks.iter().collect();
        v.sort_by(|a, b| {
            a.done
                .cmp(&b.done)
                .then_with(|| match (a.date_key(), b.date_key()) {
                    (Some(x), Some(y)) => x.cmp(y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                })
                .then_with(|| a.id.cmp(&b.id))
        });
        v
    }

    pub fn add(&mut self, text: String, day: String, reminder: bool, repeat: Repeat) -> &Task {
        let id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.tasks.push(Task {
            id,
            text,
            day,
            reminder,
            done: false,
            repeat,
        });
        self.tasks.last().unwrap()
    }

    pub fn remove(&mut self, id: u32) -> Result<Task, StoreError> {
        let idx = self.index_of(id)?;
        Ok(self.tasks.remove(idx))
    }

    /// Flip the reminder flag, returning the updated task.
    pub fn toggle_reminder(&mut self, id: u32) -> Result<&Task, StoreError> {
        let t = self.get_mut(id)?;
        t.reminder = !t.reminder;
        Ok(t)
    }

    /// Complete or reopen a task, returning the updated task.
    ///
    /// Completing a repeating task whose day is in `YYYY-MM-DD[ HH:MM]` form does
    /// not mark it done: it moves to its next day (see [`next_occurrence`], with
    /// `today` as `YYYY-MM-DD`) and stays open. Anything else just flips `done`.
    pub fn toggle_done(&mut self, id: u32, today: &str) -> Result<&Task, StoreError> {
        let t = self.get_mut(id)?;
        if !t.done
            && let Some(next) = next_occurrence(&t.day, t.repeat, today)
        {
            t.day = next;
        } else {
            t.done = !t.done;
        }
        Ok(t)
    }

    /// Change a task's text, day and/or repeat; `None` leaves that field as it is.
    pub fn update(
        &mut self,
        id: u32,
        text: Option<String>,
        day: Option<String>,
        repeat: Option<Repeat>,
    ) -> Result<&Task, StoreError> {
        let t = self.get_mut(id)?;
        if let Some(text) = text {
            t.text = text;
        }
        if let Some(day) = day {
            t.day = day;
        }
        if let Some(repeat) = repeat {
            t.repeat = repeat;
        }
        Ok(t)
    }

    pub fn clear(&mut self) -> usize {
        let n = self.tasks.len();
        self.tasks.clear();
        n
    }

    /// Delete all finished tasks, returning how many were removed.
    pub fn clear_done(&mut self) -> usize {
        let before = self.tasks.len();
        self.tasks.retain(|t| !t.done);
        before - self.tasks.len()
    }

    fn get_mut(&mut self, id: u32) -> Result<&mut Task, StoreError> {
        let idx = self.index_of(id)?;
        Ok(&mut self.tasks[idx])
    }

    fn index_of(&self, id: u32) -> Result<usize, StoreError> {
        self.tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(StoreError::NotFound(id))
    }
}

/// A [`TaskList`] backed by a file on disk.
pub struct TaskStore {
    path: PathBuf,
    pub list: TaskList,
}

impl TaskStore {
    /// Load tasks from `path`. A missing file is treated as an empty list.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let list = match fs::read_to_string(&path) {
            Ok(s) => TaskList::from_tsv(&s)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => TaskList::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(TaskStore { path, list })
    }

    /// Write tasks atomically (temp file + rename).
    pub fn save(&self) -> Result<(), StoreError> {
        if let Some(dir) = self.path.parent()
            && !dir.as_os_str().is_empty()
        {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, self.list.to_tsv())?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

// ---------- storage format ----------

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(o) => return Err(format!("unknown escape \\{o}")),
            None => return Err("trailing backslash".into()),
        }
    }
    Ok(out)
}

fn flag(s: &str, name: &str) -> Result<bool, String> {
    match s {
        "0" => Ok(false),
        "1" => Ok(true),
        o => Err(format!("bad {name} flag {o:?}")),
    }
}

pub fn serialize(tasks: &[Task]) -> String {
    tasks
        .iter()
        .map(|t| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                t.id,
                u8::from(t.reminder),
                u8::from(t.done),
                t.repeat.as_str(),
                escape(&t.day),
                escape(&t.text)
            )
        })
        .collect()
}

pub fn parse(s: &str) -> Result<Vec<Task>, StoreError> {
    let mut tasks = Vec::new();
    for (i, line) in s.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let err = |msg: String| StoreError::Parse { line: i + 1, msg };
        // Fields never contain raw tabs (they are escaped), so the field count
        // tells the three format versions apart.
        let parts: Vec<&str> = line.split('\t').collect();
        let (id, reminder, done, repeat, day, text) = match parts[..] {
            [id, r, d, rep, day, text] => (id, r, Some(d), Some(rep), day, text),
            [id, r, d, day, text] => (id, r, Some(d), None, day, text),
            [id, r, day, text] => (id, r, None, None, day, text),
            _ => {
                return Err(err(format!(
                    "expected 4, 5 or 6 fields, got {}",
                    parts.len()
                )));
            }
        };
        tasks.push(Task {
            id: id.parse().map_err(|e| err(format!("bad id {id:?}: {e}")))?,
            reminder: flag(reminder, "reminder").map_err(err)?,
            done: done
                .map(|d| flag(d, "done"))
                .transpose()
                .map_err(err)?
                .unwrap_or(false),
            repeat: repeat
                .map(|r| Repeat::parse(r).ok_or_else(|| format!("bad repeat value {r:?}")))
                .transpose()
                .map_err(err)?
                .unwrap_or_default(),
            day: unescape(day).map_err(err)?,
            text: unescape(text).map_err(err)?,
        });
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("task-tracker-test-{}-{name}", std::process::id()))
    }

    fn task(id: u32, day: &str, done: bool) -> Task {
        Task {
            id,
            text: format!("t{id}"),
            day: day.into(),
            reminder: false,
            done,
            repeat: Repeat::Never,
        }
    }

    fn add(list: &mut TaskList, text: &str, day: &str) -> u32 {
        list.add(text.into(), day.into(), false, Repeat::Never).id
    }

    #[test]
    fn roundtrip_with_special_chars() {
        let tasks = vec![
            Task {
                id: 1,
                text: "a\tb\nc\\d".into(),
                day: "Mon 10:00".into(),
                reminder: true,
                done: true,
                repeat: Repeat::Weekly,
            },
            task(7, "", false),
        ];
        assert_eq!(parse(&serialize(&tasks)).unwrap(), tasks);
    }

    #[test]
    fn reads_old_formats() {
        let four = parse("3\t1\tFeb 5th\tDoctors\n").unwrap();
        assert_eq!(four[0].id, 3);
        assert!(four[0].reminder && !four[0].done);
        assert_eq!(four[0].repeat, Repeat::Never);
        assert_eq!(
            (four[0].day.as_str(), four[0].text.as_str()),
            ("Feb 5th", "Doctors")
        );

        let five = parse("4\t0\t1\t2026-01-01\tDone one\n").unwrap();
        assert!(five[0].done);
        assert_eq!(five[0].repeat, Repeat::Never);
    }

    #[test]
    fn rejects_corrupt_line() {
        assert!(matches!(
            parse("x\t0\t\tfoo"),
            Err(StoreError::Parse { line: 1, .. })
        ));
        assert!(matches!(
            parse("1\t2\t\tfoo"),
            Err(StoreError::Parse { .. })
        ));
        assert!(matches!(
            parse("1\t0\t7\t\tfoo"),
            Err(StoreError::Parse { .. })
        ));
        assert!(matches!(
            parse("1\t0\t0\tyearly\t\tfoo"),
            Err(StoreError::Parse { .. })
        ));
        assert!(matches!(parse("1\t0"), Err(StoreError::Parse { .. })));
        assert!(matches!(
            parse("1\t0\t0\t\t\tfoo\textra"),
            Err(StoreError::Parse { .. })
        ));
    }

    #[test]
    fn ids_increase_after_removal() {
        let mut s = TaskList::new();
        add(&mut s, "a", "");
        add(&mut s, "b", "");
        s.remove(1).unwrap();
        assert_eq!(add(&mut s, "c", ""), 3);
    }

    #[test]
    fn toggles_and_missing_id() {
        let mut s = TaskList::new();
        add(&mut s, "a", "");
        assert!(s.toggle_reminder(1).unwrap().reminder);
        assert!(s.toggle_done(1, "2026-01-01").unwrap().done);
        assert!(!s.toggle_done(1, "2026-01-01").unwrap().done);
        assert!(matches!(s.toggle_reminder(9), Err(StoreError::NotFound(9))));
        assert!(matches!(s.toggle_done(9, ""), Err(StoreError::NotFound(9))));
        assert!(matches!(s.remove(9), Err(StoreError::NotFound(9))));
        assert!(matches!(
            s.update(9, None, None, None),
            Err(StoreError::NotFound(9))
        ));
    }

    #[test]
    fn update_changes_only_given_fields() {
        let mut s = TaskList::new();
        s.add("a".into(), "2026-01-01".into(), true, Repeat::Daily);
        let t = s.update(1, Some("b".into()), None, None).unwrap();
        assert_eq!(
            (t.text.as_str(), t.day.as_str(), t.repeat),
            ("b", "2026-01-01", Repeat::Daily)
        );
        let t = s
            .update(1, None, Some("".into()), Some(Repeat::Never))
            .unwrap();
        assert_eq!(
            (t.text.as_str(), t.day.as_str(), t.repeat),
            ("b", "", Repeat::Never)
        );
        assert!(t.reminder);
    }

    #[test]
    fn clear_done_keeps_open_tasks() {
        let mut s = TaskList::new();
        add(&mut s, "a", "");
        add(&mut s, "b", "");
        s.toggle_done(1, "").unwrap();
        assert_eq!(s.clear_done(), 1);
        assert_eq!(s.tasks().iter().map(|t| t.id).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn iso_day_detection() {
        for ok in [
            "2026-02-05",
            "2026-02-05 14:30",
            "1999-12-31 00:00",
            "2024-02-29",
        ] {
            assert!(is_iso_day(ok), "{ok}");
        }
        for bad in [
            "",
            "Feb 5th",
            "2026-2-5",
            "2026-13-01",
            "2026-02-05 24:00",
            "2026-02-05T14:30",
            "2026-02-05 14:3",
            "2026-02-29",
            "2026-04-31",
            "2026-00-10",
            "2026-01-00",
        ] {
            assert!(!is_iso_day(bad), "{bad}");
        }
    }

    #[test]
    fn civil_date_conversions() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        for z in [-800_000, -1, 0, 1, 11_016, 11_017, 20_000, 2_932_896] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(iso_date_from_unix_secs(0), "1970-01-01");
        assert_eq!(iso_date_from_unix_secs(86_400 * 365 - 1), "1970-12-31");
        assert_eq!(iso_date_from_unix_secs(86_400 * 365), "1971-01-01");
    }

    #[test]
    fn next_occurrence_steps() {
        use Repeat::*;
        let n = |day, r, today| next_occurrence(day, r, today);
        assert_eq!(
            n("2026-11-10", Daily, "2026-11-10").as_deref(),
            Some("2026-11-11")
        );
        assert_eq!(
            n("2026-11-10 09:30", Daily, "2026-11-10").as_deref(),
            Some("2026-11-11 09:30")
        );
        assert_eq!(
            n("2026-11-10", Weekly, "2026-11-10").as_deref(),
            Some("2026-11-17")
        );
        assert_eq!(
            n("2026-12-15", Monthly, "2026-12-15").as_deref(),
            Some("2027-01-15")
        );
        // leap years and month ends
        assert_eq!(
            n("2028-02-29", Daily, "2028-02-29").as_deref(),
            Some("2028-03-01")
        );
        assert_eq!(
            n("2024-01-31", Monthly, "2024-01-31").as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(
            n("2026-01-31", Monthly, "2026-01-31").as_deref(),
            Some("2026-02-28")
        );
    }

    #[test]
    fn next_occurrence_catches_up_past_today() {
        use Repeat::*;
        let n = |day, r, today| next_occurrence(day, r, today);
        // overdue: lands after today, not in the past
        assert_eq!(
            n("2026-11-01", Daily, "2026-11-10").as_deref(),
            Some("2026-11-11")
        );
        assert_eq!(
            n("2026-11-03", Weekly, "2026-11-10").as_deref(),
            Some("2026-11-17")
        );
        assert_eq!(
            n("2026-01-31", Monthly, "2026-03-01").as_deref(),
            Some("2026-03-31")
        );
        // completed early: still moves a whole step
        assert_eq!(
            n("2026-11-20", Weekly, "2026-11-10").as_deref(),
            Some("2026-11-27")
        );
        assert_eq!(
            n("2026-11-20", Daily, "2026-11-10").as_deref(),
            Some("2026-11-21")
        );
        // unusable `today`: one step only
        assert_eq!(n("2026-11-01", Daily, "").as_deref(), Some("2026-11-02"));
        assert_eq!(
            n("2026-11-01", Daily, "soon").as_deref(),
            Some("2026-11-02")
        );
    }

    #[test]
    fn next_occurrence_none_cases() {
        assert_eq!(
            next_occurrence("2026-11-10", Repeat::Never, "2026-11-10"),
            None
        );
        assert_eq!(
            next_occurrence("Feb 5th", Repeat::Daily, "2026-11-10"),
            None
        );
        assert_eq!(next_occurrence("", Repeat::Weekly, "2026-11-10"), None);
    }

    #[test]
    fn completing_a_repeating_task_moves_it() {
        let mut s = TaskList::new();
        s.add("약".into(), "2026-11-10 08:00".into(), true, Repeat::Daily);
        let t = s.toggle_done(1, "2026-11-10").unwrap();
        assert!(!t.done);
        assert_eq!(t.day, "2026-11-11 08:00");
        assert!(t.reminder);
        // overdue by a few days: catches up
        let t = s.toggle_done(1, "2026-11-20").unwrap();
        assert_eq!(t.day, "2026-11-21 08:00");
    }

    #[test]
    fn repeating_task_without_usable_day_just_completes() {
        let mut s = TaskList::new();
        s.add("a".into(), "Mondays".into(), false, Repeat::Weekly);
        s.add("b".into(), "".into(), false, Repeat::Daily);
        assert!(s.toggle_done(1, "2026-11-10").unwrap().done);
        assert!(s.toggle_done(2, "2026-11-10").unwrap().done);
        // and reopening a done repeating task does not advance it
        let t = s.toggle_done(1, "2026-11-10").unwrap();
        assert!(!t.done);
        assert_eq!(t.day, "Mondays");
    }

    #[test]
    fn search_matching() {
        let t = Task {
            id: 1,
            text: "Doctors Appointment".into(),
            day: "2026-11-10 14:30".into(),
            reminder: false,
            done: false,
            repeat: Repeat::Never,
        };
        assert!(t.matches(""));
        assert!(t.matches("   "));
        assert!(t.matches("doctor"));
        assert!(t.matches("APPOINT"));
        assert!(t.matches("doctors 2026-11"));
        assert!(t.matches("14:30 appointment"));
        assert!(!t.matches("doctors dentist"));
        assert!(!t.matches("2027"));
        let k = Task {
            text: "병원 예약".into(),
            day: String::new(),
            ..t
        };
        assert!(k.matches("병원"));
        assert!(k.matches("예약 병원"));
        assert!(!k.matches("치과"));
    }

    #[test]
    fn sorted_order() {
        let list = TaskList {
            tasks: vec![
                task(1, "Feb 5th", false),
                task(2, "2026-03-01", false),
                task(3, "2026-01-10 09:00", true),
                task(4, "2026-01-10 08:00", false),
                task(5, "", false),
                task(6, "2026-01-10", false),
            ],
        };
        let ids: Vec<u32> = list.sorted().iter().map(|t| t.id).collect();
        // open dated (by date), open undated (by id), then done
        assert_eq!(ids, [6, 4, 2, 1, 5, 3]);
    }

    #[test]
    fn save_and_reopen() {
        let path = tmp_path("save.tsv");
        let _ = fs::remove_file(&path);
        let mut s = TaskStore::open(&path).unwrap();
        assert!(s.list.tasks().is_empty());
        s.list.add(
            "Doctors appointment".into(),
            "2026-02-05 14:30".into(),
            true,
            Repeat::Monthly,
        );
        s.list.toggle_reminder(1).unwrap();
        s.save().unwrap();
        let s2 = TaskStore::open(&path).unwrap();
        assert_eq!(s2.list, s.list);
        fs::remove_file(&path).unwrap();
    }
}
