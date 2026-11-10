//! WebAssembly bindings: exposes the shared Rust `TaskList` to JavaScript.

use task_tracker::task::{Repeat, Task, TaskList};
use wasm_bindgen::prelude::*;

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// `""`, `"daily"`, `"weekly"` or `"monthly"`.
fn parse_repeat(s: &str) -> Result<Repeat, JsError> {
    Repeat::parse(s).ok_or_else(|| JsError::new("알 수 없는 반복 설정입니다"))
}

#[wasm_bindgen]
pub struct Tracker {
    list: TaskList,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Tracker {
    /// Create an empty tracker.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Tracker {
        Tracker {
            list: TaskList::new(),
        }
    }

    /// Restore from the TSV produced by [`Tracker::save`].
    pub fn load(data: &str) -> Result<Tracker, JsError> {
        let list = TaskList::from_tsv(data).map_err(js_err)?;
        Ok(Tracker { list })
    }

    /// Serialize to the same TSV format the CLI writes to `tasks.tsv`.
    pub fn save(&self) -> String {
        self.list.to_tsv()
    }

    /// Add a task and return its id. Errors on empty text.
    /// `repeat` is `""`, `"daily"`, `"weekly"` or `"monthly"`.
    pub fn add(
        &mut self,
        text: &str,
        day: &str,
        reminder: bool,
        repeat: &str,
    ) -> Result<u32, JsError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(JsError::new("할 일을 입력하세요"));
        }
        let repeat = parse_repeat(repeat)?;
        Ok(self
            .list
            .add(text.into(), day.trim().into(), reminder, repeat)
            .id)
    }

    pub fn remove(&mut self, id: u32) -> Result<(), JsError> {
        self.list.remove(id).map(|_| ()).map_err(js_err)
    }

    /// Toggle the reminder; returns the new state.
    pub fn toggle(&mut self, id: u32) -> Result<bool, JsError> {
        self.list
            .toggle_reminder(id)
            .map(|t| t.reminder)
            .map_err(js_err)
    }

    /// Complete or reopen a task; returns whether it is now done.
    /// A repeating task moves to its next day and stays open (returns `false`).
    /// `today` is `YYYY-MM-DD` in the user's local time.
    #[wasm_bindgen(js_name = toggleDone)]
    pub fn toggle_done(&mut self, id: u32, today: &str) -> Result<bool, JsError> {
        self.list
            .toggle_done(id, today)
            .map(|t| t.done)
            .map_err(js_err)
    }

    /// Replace a task's text, day and repeat. Errors on empty text.
    pub fn update(&mut self, id: u32, text: &str, day: &str, repeat: &str) -> Result<(), JsError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(JsError::new("할 일을 입력하세요"));
        }
        let repeat = parse_repeat(repeat)?;
        self.list
            .update(id, Some(text.into()), Some(day.trim().into()), Some(repeat))
            .map(|_| ())
            .map_err(js_err)
    }

    pub fn clear(&mut self) -> usize {
        self.list.clear()
    }

    #[wasm_bindgen(js_name = clearDone)]
    pub fn clear_done(&mut self) -> usize {
        self.list.clear_done()
    }

    /// Tasks as a JSON array:
    /// `[{"id":1,"text":"…","day":"…","reminder":true,"done":false,"repeat":"daily"}]`.
    /// With `sorted`, open tasks come first and dated tasks are in date order.
    /// Only tasks matching `query` (see `Task::matches`) are included; `""` matches all.
    #[wasm_bindgen(js_name = tasksJson)]
    pub fn tasks_json(&self, sorted: bool, query: &str) -> String {
        let ordered: Vec<&Task> = if sorted {
            self.list.sorted()
        } else {
            self.list.tasks().iter().collect()
        };
        tasks_to_json(ordered.into_iter().filter(|t| t.matches(query)))
    }
}

fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn tasks_to_json<'a>(tasks: impl IntoIterator<Item = &'a Task>) -> String {
    let mut out = String::from("[");
    for (i, t) in tasks.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"id\":{},\"text\":", t.id));
        json_str(&t.text, &mut out);
        out.push_str(",\"day\":");
        json_str(&t.day, &mut out);
        out.push_str(&format!(
            ",\"reminder\":{},\"done\":{},\"repeat\":\"{}\"}}",
            t.reminder, t.done, t.repeat
        ));
    }
    out.push(']');
    out
}

// Note: `JsError` can only be created inside a browser (WASM), so these native tests
// cover the success paths; the error paths are exercised in the browser tests.
#[cfg(test)]
mod tests {
    use super::*;

    fn add(t: &mut Tracker, text: &str, day: &str, repeat: Repeat) {
        t.list.add(text.into(), day.into(), false, repeat);
    }

    #[test]
    fn json_escapes() {
        let tasks = [Task {
            id: 2,
            text: "say \"hi\"\n\u{1}".into(),
            day: "월".into(),
            reminder: true,
            done: false,
            repeat: Repeat::Weekly,
        }];
        assert_eq!(
            tasks_to_json(&tasks),
            r#"[{"id":2,"text":"say \"hi\"\n\u0001","day":"월","reminder":true,"done":false,"repeat":"weekly"}]"#
        );
        assert_eq!(tasks_to_json(&[]), "[]");
    }

    #[test]
    fn sorted_json_and_update() {
        let mut t = Tracker::new();
        add(&mut t, "later", "2026-05-01", Repeat::Never);
        add(&mut t, "sooner", "2026-01-01", Repeat::Never);
        assert!(t.tasks_json(true, "").find("sooner") < t.tasks_json(true, "").find("later"));
        assert!(t.tasks_json(false, "").find("later") < t.tasks_json(false, "").find("sooner"));
        t.update(1, " renamed ", "", "daily").unwrap();
        assert!(
            t.tasks_json(false, "")
                .contains(r#""text":"renamed","day":"""#)
        );
        assert!(t.tasks_json(false, "").contains(r#""repeat":"daily""#));
    }

    #[test]
    fn query_filters_json() {
        let mut t = Tracker::new();
        add(&mut t, "later", "2026-05-01", Repeat::Never);
        add(&mut t, "sooner", "2026-01-01", Repeat::Never);
        let found = t.tasks_json(true, "SOON");
        assert!(found.contains("sooner") && !found.contains("later"));
        assert!(t.tasks_json(false, "2026-05").contains("later"));
        assert_eq!(t.tasks_json(false, "nothing"), "[]");
    }

    #[test]
    fn done_moves_repeating_tasks() {
        let mut t = Tracker::new();
        t.add("약", "2026-11-10", false, "daily").unwrap();
        t.add("메모", "", false, "").unwrap();
        assert!(!t.toggle_done(1, "2026-11-10").unwrap());
        assert!(t.tasks_json(false, "").contains(r#""day":"2026-11-11""#));
        assert!(t.toggle_done(2, "2026-11-10").unwrap());
    }

    #[test]
    fn save_load_roundtrip() {
        let mut t = Tracker::new();
        add(&mut t, "a", "2026-01-01", Repeat::Monthly);
        let u = Tracker {
            list: TaskList::from_tsv(&t.save()).unwrap(),
        };
        assert_eq!(u.tasks_json(false, ""), t.tasks_json(false, ""));
    }
}
