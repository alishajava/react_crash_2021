//! Task model and on-disk storage.
//!
//! Tasks are stored one per line as tab-separated values:
//! `id \t reminder(0|1) \t day \t text`. Tabs, newlines and backslashes
//! inside fields are escaped so any text round-trips safely.

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
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bell = if self.reminder { "🔔" } else { "  " };
        write!(f, "{bell} [{:>3}] {}", self.id, self.text)?;
        if !self.day.is_empty() {
            write!(f, "  — {}", self.day)?;
        }
        Ok(())
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

/// An in-memory list of tasks backed by a file.
pub struct TaskStore {
    path: PathBuf,
    tasks: Vec<Task>,
}

impl TaskStore {
    /// Load tasks from `path`. A missing file is treated as an empty list.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let tasks = match fs::read_to_string(&path) {
            Ok(s) => parse(&s)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(TaskStore { path, tasks })
    }

    /// Write tasks atomically (temp file + rename).
    pub fn save(&self) -> Result<(), StoreError> {
        if let Some(dir) = self.path.parent()
            && !dir.as_os_str().is_empty()
        {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, serialize(&self.tasks))?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn add(&mut self, text: String, day: String, reminder: bool) -> &Task {
        let id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.tasks.push(Task { id, text, day, reminder });
        self.tasks.last().unwrap()
    }

    pub fn remove(&mut self, id: u32) -> Result<Task, StoreError> {
        let idx = self.index_of(id)?;
        Ok(self.tasks.remove(idx))
    }

    /// Flip the reminder flag, returning the updated task.
    pub fn toggle_reminder(&mut self, id: u32) -> Result<&Task, StoreError> {
        let idx = self.index_of(id)?;
        let t = &mut self.tasks[idx];
        t.reminder = !t.reminder;
        Ok(t)
    }

    pub fn clear(&mut self) -> usize {
        let n = self.tasks.len();
        self.tasks.clear();
        n
    }

    fn index_of(&self, id: u32) -> Result<usize, StoreError> {
        self.tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(StoreError::NotFound(id))
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

pub fn serialize(tasks: &[Task]) -> String {
    tasks
        .iter()
        .map(|t| {
            format!(
                "{}\t{}\t{}\t{}\n",
                t.id,
                u8::from(t.reminder),
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
        let parts: Vec<&str> = line.splitn(4, '\t').collect();
        if parts.len() != 4 {
            return Err(err(format!("expected 4 fields, got {}", parts.len())));
        }
        let id = parts[0]
            .parse()
            .map_err(|e| err(format!("bad id {:?}: {e}", parts[0])))?;
        let reminder = match parts[1] {
            "0" => false,
            "1" => true,
            o => return Err(err(format!("bad reminder flag {o:?}"))),
        };
        tasks.push(Task {
            id,
            reminder,
            day: unescape(parts[2]).map_err(err)?,
            text: unescape(parts[3]).map_err(err)?,
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

    #[test]
    fn roundtrip_with_special_chars() {
        let tasks = vec![
            Task { id: 1, text: "a\tb\nc\\d".into(), day: "Mon 10:00".into(), reminder: true },
            Task { id: 7, text: "plain".into(), day: String::new(), reminder: false },
        ];
        assert_eq!(parse(&serialize(&tasks)).unwrap(), tasks);
    }

    #[test]
    fn rejects_corrupt_line() {
        assert!(matches!(parse("x\t0\t\tfoo"), Err(StoreError::Parse { line: 1, .. })));
        assert!(matches!(parse("1\t2\t\tfoo"), Err(StoreError::Parse { .. })));
        assert!(matches!(parse("1\t0"), Err(StoreError::Parse { .. })));
    }

    #[test]
    fn ids_increase_after_removal() {
        let mut s = TaskStore { path: tmp_path("ids"), tasks: vec![] };
        s.add("a".into(), "".into(), false);
        s.add("b".into(), "".into(), false);
        s.remove(1).unwrap();
        assert_eq!(s.add("c".into(), "".into(), false).id, 3);
    }

    #[test]
    fn toggle_and_missing_id() {
        let mut s = TaskStore { path: tmp_path("toggle"), tasks: vec![] };
        s.add("a".into(), "".into(), false);
        assert!(s.toggle_reminder(1).unwrap().reminder);
        assert!(matches!(s.toggle_reminder(9), Err(StoreError::NotFound(9))));
        assert!(matches!(s.remove(9), Err(StoreError::NotFound(9))));
    }

    #[test]
    fn save_and_reopen() {
        let path = tmp_path("save.tsv");
        let _ = fs::remove_file(&path);
        let mut s = TaskStore::open(&path).unwrap();
        assert!(s.tasks().is_empty());
        s.add("Doctors appointment".into(), "Feb 5th at 2:30pm".into(), true);
        s.save().unwrap();
        let s2 = TaskStore::open(&path).unwrap();
        assert_eq!(s2.tasks(), s.tasks());
        fs::remove_file(&path).unwrap();
    }
}
