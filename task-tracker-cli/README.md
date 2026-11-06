# task-tracker (Rust CLI)

A small command-line companion to the React Task Tracker in this repo, written in
Rust with no external dependencies. Each task has text, a day/time, and a reminder flag.

## Build & run

```sh
cd task-tracker-cli
cargo run -- add "Doctors appointment" --day "Feb 5th at 2:30pm" --reminder
cargo run -- add "Meeting at school" -d "Feb 6th"
cargo run -- list
cargo run -- toggle 2       # toggle reminder
cargo run -- list -r        # only tasks with reminders
cargo run -- rm 1
cargo run -- clear
```

Tasks are saved to `./tasks.tsv` by default; set `TASK_TRACKER_FILE` to use another path.
The file is plain tab-separated text and is written atomically.

## Test

```sh
cargo test
cargo clippy -- -D warnings
```
