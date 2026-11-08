//! WebAssembly bindings: exposes the shared Rust `TaskList` to JavaScript.

use task_tracker::task::{Task, TaskList};
use wasm_bindgen::prelude::*;

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
        let list = TaskList::from_tsv(data).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Tracker { list })
    }

    /// Serialize to the same TSV format the CLI writes to `tasks.tsv`.
    pub fn save(&self) -> String {
        self.list.to_tsv()
    }

    /// Add a task and return its id. Errors on empty text.
    pub fn add(&mut self, text: &str, day: &str, reminder: bool) -> Result<u32, JsError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(JsError::new("할 일을 입력하세요"));
        }
        Ok(self.list.add(text.into(), day.trim().into(), reminder).id)
    }

    pub fn remove(&mut self, id: u32) -> Result<(), JsError> {
        self.list
            .remove(id)
            .map(|_| ())
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Toggle the reminder; returns the new state.
    pub fn toggle(&mut self, id: u32) -> Result<bool, JsError> {
        self.list
            .toggle_reminder(id)
            .map(|t| t.reminder)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Toggle done; returns the new state.
    #[wasm_bindgen(js_name = toggleDone)]
    pub fn toggle_done(&mut self, id: u32) -> Result<bool, JsError> {
        self.list
            .toggle_done(id)
            .map(|t| t.done)
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Replace a task's text and day. Errors on empty text.
    pub fn update(&mut self, id: u32, text: &str, day: &str) -> Result<(), JsError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(JsError::new("할 일을 입력하세요"));
        }
        self.list
            .update(id, Some(text.into()), Some(day.trim().into()))
            .map(|_| ())
            .map_err(|e| JsError::new(&e.to_string()))
    }

    pub fn clear(&mut self) -> usize {
        self.list.clear()
    }

    #[wasm_bindgen(js_name = clearDone)]
    pub fn clear_done(&mut self) -> usize {
        self.list.clear_done()
    }

    /// Tasks as a JSON array:
    /// `[{"id":1,"text":"…","day":"…","reminder":true,"done":false}]`.
    /// With `sorted`, open tasks come first and dated tasks are in date order.
    #[wasm_bindgen(js_name = tasksJson)]
    pub fn tasks_json(&self, sorted: bool) -> String {
        if sorted {
            tasks_to_json(self.list.sorted())
        } else {
            tasks_to_json(self.list.tasks())
        }
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
            ",\"reminder\":{},\"done\":{}}}",
            t.reminder, t.done
        ));
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escapes() {
        let tasks = [Task {
            id: 2,
            text: "say \"hi\"\n\u{1}".into(),
            day: "월".into(),
            reminder: true,
            done: false,
        }];
        assert_eq!(
            tasks_to_json(&tasks),
            r#"[{"id":2,"text":"say \"hi\"\n\u0001","day":"월","reminder":true,"done":false}]"#
        );
        assert_eq!(tasks_to_json(&[]), "[]");
    }

    #[test]
    fn sorted_json_and_update() {
        let mut t = Tracker::new();
        t.list.add("later".into(), "2026-05-01".into(), false);
        t.list.add("sooner".into(), "2026-01-01".into(), false);
        assert!(t.tasks_json(true).find("sooner") < t.tasks_json(true).find("later"));
        assert!(t.tasks_json(false).find("later") < t.tasks_json(false).find("sooner"));
        t.update(1, " renamed ", "").unwrap();
        assert!(t.tasks_json(false).contains(r#""text":"renamed","day":"""#));
    }

    #[test]
    fn save_load_roundtrip() {
        let mut t = Tracker::new();
        t.list.add("a".into(), "Mon".into(), false);
        let u = Tracker {
            list: TaskList::from_tsv(&t.save()).unwrap(),
        };
        assert_eq!(u.tasks_json(false), t.tasks_json(false));
    }
}
