//! task-tracker: a tiny command-line companion to the React Task Tracker.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use task_tracker::task::TaskStore;

const USAGE: &str = "\
task-tracker — manage tasks from the terminal

USAGE:
    task-tracker <COMMAND> [ARGS]

COMMANDS:
    add <text> [--day <day>] [--reminder]   Add a task
    list [--reminders]                      List tasks (optionally only reminders)
    toggle <id>                             Toggle a task's reminder
    rm <id>                                 Delete a task
    clear                                   Delete all tasks
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
            let mut store = TaskStore::open(store_path())?;
            println!("Added: {}", store.list.add(text, day, reminder));
            store.save()?;
        }

        "list" | "ls" => {
            let only_reminders = rest.iter().any(|a| a == "--reminders" || a == "-r");
            let store = TaskStore::open(store_path())?;
            let shown: Vec<_> = store
                .list
                .tasks()
                .iter()
                .filter(|t| !only_reminders || t.reminder)
                .collect();
            if shown.is_empty() {
                println!("No tasks to show");
            } else {
                for t in shown {
                    println!("{t}");
                }
            }
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
            println!("Deleted {} task(s)", store.list.clear());
            store.save()?;
        }

        other => return Err(format!("unknown command {other:?} (try `help`)").into()),
    }
    Ok(())
}
