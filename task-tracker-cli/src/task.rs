//! Task model and on-disk storage.
//!
//! Tasks are stored one per line as tab-separated values:
//! `id \t reminder(0|1) \t done(0|1) \t day \t text`. Tabs, newlines and
//! backslashes inside fields are escaped so any text round-trips safely.
//! Older files without the `done` column (4 fields) are still accepted.
//!
//! `day` is free text, but the recommended form is `YYYY-MM-DD` or
//! `YYYY-MM-DD HH:MM`; days in that form are sorted chronologically.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: u32,
    pub text: String,
    pub day: String,
    pub reminder: bool,
    pub done: bool,
}

impl Task {
    /// The day as a sortable key, if it is in `YYYY-MM-DD[ HH:MM]` form.
    pub fn date_key(&self) -> Option<&str> {
        is_iso_day(&self.day).then_some(self.day.as_str())
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
        Ok(())
    }
}

/// `YYYY-MM-DD` or `YYYY-MM-DD HH:MM` with plausible month/day/hour/minute.
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
        && (1..=31).contains(&num(8..10));
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

    pub fn add(&mut self, text: String, day: String, reminder: bool) -> &Task {
        let id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.tasks.push(Task {
            id,
            text,
            day,
            reminder,
            done: false,
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

    /// Flip the done flag, returning the updated task.
    pub fn toggle_done(&mut self, id: u32) -> Result<&Task, StoreError> {
        let t = self.get_mut(id)?;
        t.done = !t.done;
        Ok(t)
    }

    /// Change a task's text and/or day; `None` leaves that field as it is.
    pub fn update(
        &mut self,
        id: u32,
        text: Option<String>,
        day: Option<String>,
    ) -> Result<&Task, StoreError> {
        let t = self.get_mut(id)?;
        if let Some(text) = text {
            t.text = text;
        }
        if let Some(day) = day {
            t.day = day;
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
                "{}\t{}\t{}\t{}\t{}\n",
                t.id,
                u8::from(t.reminder),
                u8::from(t.done),
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
        // tells the old 4-column format apart from the current 5-column one.
        let parts: Vec<&str> = line.split('\t').collect();
        let (id, reminder, done, day, text) = match parts[..] {
            [id, r, d, day, text] => (id, r, Some(d), day, text),
            [id, r, day, text] => (id, r, None, day, text),
            _ => return Err(err(format!("expected 4 or 5 fields, got {}", parts.len()))),
        };
        tasks.push(Task {
            id: id.parse().map_err(|e| err(format!("bad id {id:?}: {e}")))?,
            reminder: flag(reminder, "reminder").map_err(err)?,
            done: done
                .map(|d| flag(d, "done"))
                .transpose()
                .map_err(err)?
                .unwrap_or(false),
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
        }
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
            },
            task(7, "", false),
        ];
        assert_eq!(parse(&serialize(&tasks)).unwrap(), tasks);
    }

    #[test]
    fn reads_old_four_column_format() {
        let tasks = parse("3\t1\tFeb 5th\tDoctors\n").unwrap();
        assert_eq!(tasks[0].id, 3);
        assert!(tasks[0].reminder && !tasks[0].done);
        assert_eq!(
            (tasks[0].day.as_str(), tasks[0].text.as_str()),
            ("Feb 5th", "Doctors")
        );
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
        assert!(matches!(parse("1\t0"), Err(StoreError::Parse { .. })));
    }

    #[test]
    fn ids_increase_after_removal() {
        let mut s = TaskList::new();
        s.add("a".into(), "".into(), false);
        s.add("b".into(), "".into(), false);
        s.remove(1).unwrap();
        assert_eq!(s.add("c".into(), "".into(), false).id, 3);
    }

    #[test]
    fn toggles_and_missing_id() {
        let mut s = TaskList::new();
        s.add("a".into(), "".into(), false);
        assert!(s.toggle_reminder(1).unwrap().reminder);
        assert!(s.toggle_done(1).unwrap().done);
        assert!(!s.toggle_done(1).unwrap().done);
        assert!(matches!(s.toggle_reminder(9), Err(StoreError::NotFound(9))));
        assert!(matches!(s.toggle_done(9), Err(StoreError::NotFound(9))));
        assert!(matches!(s.remove(9), Err(StoreError::NotFound(9))));
        assert!(matches!(
            s.update(9, None, None),
            Err(StoreError::NotFound(9))
        ));
    }

    #[test]
    fn update_changes_only_given_fields() {
        let mut s = TaskList::new();
        s.add("a".into(), "2026-01-01".into(), true);
        let t = s.update(1, Some("b".into()), None).unwrap();
        assert_eq!(
            (t.text.as_str(), t.day.as_str(), t.reminder),
            ("b", "2026-01-01", true)
        );
        let t = s.update(1, None, Some("".into())).unwrap();
        assert_eq!((t.text.as_str(), t.day.as_str()), ("b", ""));
    }

    #[test]
    fn clear_done_keeps_open_tasks() {
        let mut s = TaskList::new();
        s.add("a".into(), "".into(), false);
        s.add("b".into(), "".into(), false);
        s.toggle_done(1).unwrap();
        assert_eq!(s.clear_done(), 1);
        assert_eq!(s.tasks().iter().map(|t| t.id).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn iso_day_detection() {
        for ok in ["2026-02-05", "2026-02-05 14:30", "1999-12-31 00:00"] {
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
        ] {
            assert!(!is_iso_day(bad), "{bad}");
        }
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
        );
        s.list.toggle_done(1).unwrap();
        s.save().unwrap();
        let s2 = TaskStore::open(&path).unwrap();
        assert_eq!(s2.list, s.list);
        fs::remove_file(&path).unwrap();
    }
}
