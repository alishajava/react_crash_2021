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
        Tracker { list: TaskList::new() }
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

    pub fn clear(&mut self) -> usize {
        self.list.clear()
    }

    /// All tasks as a JSON array: `[{"id":1,"text":"…","day":"…","reminder":true}]`.
    #[wasm_bindgen(js_name = tasksJson)]
    pub fn tasks_json(&self) -> String {
        tasks_to_json(self.list.tasks())
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

fn tasks_to_json(tasks: &[Task]) -> String {
    let mut out = String::from("[");
    for (i, t) in tasks.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"id\":{},\"text\":", t.id));
        json_str(&t.text, &mut out);
        out.push_str(",\"day\":");
        json_str(&t.day, &mut out);
        out.push_str(&format!(",\"reminder\":{}}}", t.reminder));
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escapes() {
        let tasks = [Task { id: 2, text: "say \"hi\"\n\u{1}".into(), day: "월".into(), reminder: true }];
        assert_eq!(
            tasks_to_json(&tasks),
            r#"[{"id":2,"text":"say \"hi\"\n\u0001","day":"월","reminder":true}]"#
        );
        assert_eq!(tasks_to_json(&[]), "[]");
    }

    #[test]
    fn save_load_roundtrip() {
        let mut t = Tracker::new();
        t.list.add("a".into(), "Mon".into(), false);
        let u = Tracker { list: TaskList::from_tsv(&t.save()).unwrap() };
        assert_eq!(u.tasks_json(), t.tasks_json());
    }
}
