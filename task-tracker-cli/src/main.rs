//! task-tracker: a tiny command-line companion to the React Task Tracker.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use task_tracker::task::{TaskStore, is_iso_day};

const USAGE: &str = "\
task-tracker — manage tasks from the terminal

USAGE:
    task-tracker <COMMAND> [ARGS]

COMMANDS:
    add <text> [--day <day>] [--reminder]   Add a task (day e.g. '2026-02-05 14:30')
    list [OPTIONS]                          List tasks: open ones first, by date
        --reminders, -r                       only tasks with a reminder
        --open, -o                            hide finished tasks
        --by-id                               in the order they were added
    done <id>                               Mark a task done / not done
    edit <id> [<text>] [--day <day>]        Change a task's text and/or day
    toggle <id>                             Toggle a task's reminder
    rm <id>                                 Delete a task
    clear [--done]                          Delete all tasks (or only finished ones)
    help                                    Show this message

The task file defaults to ./tasks.tsv; override it with TASK_TRACKER_FILE.";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn store_path() -> PathBuf {
    env::var_os("TASK_TRACKER_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tasks.tsv"))
}

fn parse_id(arg: Option<&String>) -> Result<u32, String> {
    let s = arg.ok_or("missing task id")?;
    s.parse().map_err(|_| format!("invalid task id {s:?}"))
}

/// Free-text days are allowed, but only `YYYY-MM-DD[ HH:MM]` sorts by date.
fn warn_day(day: &str) {
    if !day.is_empty() && !is_iso_day(day) {
        eprintln!(
            "note: {day:?} is not YYYY-MM-DD[ HH:MM]; it will be kept as text and not sorted by date"
        );
    }
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let Some((cmd, rest)) = args.split_first() else {
        println!("{USAGE}");
        return Ok(());
    };

    match cmd.as_str() {
        "help" | "-h" | "--help" => println!("{USAGE}"),

        "add" => {
            let mut text = Vec::new();
            let mut day = String::new();
            let mut reminder = false;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--day" | "-d" => day = it.next().ok_or("--day needs a value")?.clone(),
                    "--reminder" | "-r" => reminder = true,
                    _ => text.push(a.as_str()),
                }
            }
            let text = text.join(" ");
            if text.trim().is_empty() {
                return Err("please add a task description".into());
            }
            warn_day(&day);
            let mut store = TaskStore::open(store_path())?;
            println!("Added: {}", store.list.add(text, day, reminder));
            store.save()?;
        }

        "list" | "ls" => {
            let has = |names: &[&str]| rest.iter().any(|a| names.contains(&a.as_str()));
            let only_reminders = has(&["--reminders", "-r"]);
            let only_open = has(&["--open", "-o"]);
            let store = TaskStore::open(store_path())?;
            let ordered: Vec<_> = if has(&["--by-id"]) {
                store.list.tasks().iter().collect()
            } else {
                store.list.sorted()
            };
            let shown: Vec<_> = ordered
                .into_iter()
                .filter(|t| !only_reminders || t.reminder)
                .filter(|t| !only_open || !t.done)
                .collect();
            if shown.is_empty() {
                println!("No tasks to show");
            } else {
                for t in shown {
                    println!("{t}");
                }
            }
        }

        "done" => {
            let id = parse_id(rest.first())?;
            let mut store = TaskStore::open(store_path())?;
            let t = store.list.toggle_done(id)?;
            println!("{}: {t}", if t.done { "Done" } else { "Not done" });
            store.save()?;
        }

        "edit" => {
            let id = parse_id(rest.first())?;
            let mut text = Vec::new();
            let mut day = None;
            let mut it = rest[1..].iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--day" | "-d" => day = Some(it.next().ok_or("--day needs a value")?.clone()),
                    _ => text.push(a.as_str()),
                }
            }
            let text = (!text.is_empty()).then(|| text.join(" "));
            if text.is_none() && day.is_none() {
                return Err("nothing to change: give new text and/or --day".into());
            }
            if text.as_deref().is_some_and(|t| t.trim().is_empty()) {
                return Err("task text cannot be empty".into());
            }
            if let Some(d) = &day {
                warn_day(d);
            }
            let mut store = TaskStore::open(store_path())?;
            println!("Updated: {}", store.list.update(id, text, day)?);
            store.save()?;
        }

        "toggle" => {
            let id = parse_id(rest.first())?;
            let mut store = TaskStore::open(store_path())?;
            let t = store.list.toggle_reminder(id)?;
            println!("Reminder {}: {t}", if t.reminder { "on" } else { "off" });
            store.save()?;
        }

        "rm" | "delete" => {
            let id = parse_id(rest.first())?;
            let mut store = TaskStore::open(store_path())?;
            println!("Deleted: {}", store.list.remove(id)?);
            store.save()?;
        }

        "clear" => {
            let mut store = TaskStore::open(store_path())?;
            let n = if rest.iter().any(|a| a == "--done") {
                store.list.clear_done()
            } else {
                store.list.clear()
            };
            println!("Deleted {n} task(s)");
            store.save()?;
        }

        other => return Err(format!("unknown command {other:?} (try `help`)").into()),
    }
    Ok(())
}
