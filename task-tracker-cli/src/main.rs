//! task-tracker: a tiny command-line companion to the React Task Tracker.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use task_tracker::task::{Repeat, TaskStore, is_iso_day, iso_date_from_unix_secs};

const USAGE: &str = "\
task-tracker — manage tasks from the terminal

USAGE:
    task-tracker <COMMAND> [ARGS]

COMMANDS:
    add <text> [OPTIONS]                    Add a task
        --day, -d <day>                       e.g. '2026-02-05 14:30'
        --reminder, -r                        turn the reminder on
        --repeat <daily|weekly|monthly>       come back after completing (needs a YYYY-MM-DD day)
    list [WORDS] [OPTIONS]                  List tasks: open ones first, by date.
                                            Words search the text and day (all must match);
                                            `search` and `find` are the same command.
        --reminders, -r                       only tasks with a reminder
        --open, -o                            hide finished tasks
        --by-id                               in the order they were added
    done <id>                               Complete a task (repeating tasks move to their
                                            next day instead) / reopen a finished one
    edit <id> [<text>] [OPTIONS]            Change a task
        --day, -d <day>                       new day
        --repeat <none|daily|weekly|monthly>  new repeat setting
    toggle <id>                             Toggle a task's reminder
    rm <id>                                 Delete a task
    clear [--done]                          Delete all tasks (or only finished ones)
    help                                    Show this message

The task file defaults to ./tasks.tsv; override it with TASK_TRACKER_FILE.
`done` uses today's date in UTC for repeating tasks; override it with
TASK_TRACKER_TODAY=YYYY-MM-DD.";

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

/// Today as `YYYY-MM-DD`: `TASK_TRACKER_TODAY` if set, else the current UTC date.
fn today() -> String {
    if let Ok(t) = env::var("TASK_TRACKER_TODAY") {
        return t;
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    iso_date_from_unix_secs(secs)
}

fn parse_id(arg: Option<&String>) -> Result<u32, String> {
    let s = arg.ok_or("missing task id")?;
    s.parse().map_err(|_| format!("invalid task id {s:?}"))
}

fn parse_repeat(arg: Option<&String>) -> Result<Repeat, String> {
    let s = arg.ok_or("--repeat needs a value")?;
    Repeat::parse(s)
        .ok_or_else(|| format!("unknown repeat {s:?} (use none, daily, weekly or monthly)"))
}

/// Free-text days are allowed, but only `YYYY-MM-DD[ HH:MM]` sorts by date.
fn warn_day(day: &str) {
    if !day.is_empty() && !is_iso_day(day) {
        eprintln!(
            "note: {day:?} is not YYYY-MM-DD[ HH:MM]; it will be kept as text and not sorted by date"
        );
    }
}

fn warn_repeat(day: &str, repeat: Repeat) {
    if repeat != Repeat::Never && !is_iso_day(day) {
        eprintln!(
            "note: repeating needs a YYYY-MM-DD[ HH:MM] day; this task won't repeat until it has one"
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
            let mut repeat = Repeat::Never;
            let mut it = rest.iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--day" | "-d" => day = it.next().ok_or("--day needs a value")?.clone(),
                    "--reminder" | "-r" => reminder = true,
                    "--repeat" => repeat = parse_repeat(it.next())?,
                    _ => text.push(a.as_str()),
                }
            }
            let text = text.join(" ");
            if text.trim().is_empty() {
                return Err("please add a task description".into());
            }
            warn_day(&day);
            warn_repeat(&day, repeat);
            let mut store = TaskStore::open(store_path())?;
            println!("Added: {}", store.list.add(text, day, reminder, repeat));
            store.save()?;
        }

        "list" | "ls" | "search" | "find" => {
            let (mut only_reminders, mut only_open, mut by_id) = (false, false, false);
            let mut words = Vec::new();
            for a in rest {
                match a.as_str() {
                    "--reminders" | "-r" => only_reminders = true,
                    "--open" | "-o" => only_open = true,
                    "--by-id" => by_id = true,
                    o if o.starts_with('-') && o.len() > 1 => {
                        return Err(format!("unknown option {o:?} (try `help`)").into());
                    }
                    w => words.push(w),
                }
            }
            let query = words.join(" ");
            let store = TaskStore::open(store_path())?;
            let ordered: Vec<_> = if by_id {
                store.list.tasks().iter().collect()
            } else {
                store.list.sorted()
            };
            let shown: Vec<_> = ordered
                .into_iter()
                .filter(|t| t.matches(&query))
                .filter(|t| !only_reminders || t.reminder)
                .filter(|t| !only_open || !t.done)
                .collect();
            if shown.is_empty() {
                if query.is_empty() {
                    println!("No tasks to show");
                } else {
                    println!("No tasks match {query:?}");
                }
            } else {
                for t in shown {
                    println!("{t}");
                }
            }
        }

        "done" => {
            let id = parse_id(rest.first())?;
            let mut store = TaskStore::open(store_path())?;
            let was_done = store.list.get(id).is_some_and(|t| t.done);
            let t = store.list.toggle_done(id, &today())?;
            if !was_done && !t.done {
                println!("Next {}: {t}", t.repeat);
            } else if t.done {
                println!("Done: {t}");
            } else {
                println!("Reopened: {t}");
            }
            store.save()?;
        }

        "edit" => {
            let id = parse_id(rest.first())?;
            let mut text = Vec::new();
            let mut day = None;
            let mut repeat = None;
            let mut it = rest[1..].iter();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--day" | "-d" => day = Some(it.next().ok_or("--day needs a value")?.clone()),
                    "--repeat" => repeat = Some(parse_repeat(it.next())?),
                    _ => text.push(a.as_str()),
                }
            }
            let text = (!text.is_empty()).then(|| text.join(" "));
            if text.is_none() && day.is_none() && repeat.is_none() {
                return Err("nothing to change: give new text, --day and/or --repeat".into());
            }
            if text.as_deref().is_some_and(|t| t.trim().is_empty()) {
                return Err("task text cannot be empty".into());
            }
            if let Some(d) = &day {
                warn_day(d);
            }
            let mut store = TaskStore::open(store_path())?;
            let t = store.list.update(id, text, day, repeat)?;
            warn_repeat(&t.day, t.repeat);
            println!("Updated: {t}");
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
