# task-tracker (Rust CLI)

A small command-line companion to the React Task Tracker in this repo, written in
Rust with no external dependencies. Each task has text, a day/time, a reminder flag and a done flag.

## Build & run

```sh
cd task-tracker-cli
cargo run -- add "Doctors appointment" --day "2026-02-05 14:30" --reminder
cargo run -- add "Meeting at school" -d 2026-02-06
cargo run -- list            # open tasks first, sorted by date
cargo run -- list --by-id    # in the order added
cargo run -- list -r -o      # only reminders, hide finished
cargo run -- done 2          # mark done / not done
cargo run -- edit 1 Dentist --day "2026-02-07 09:00"
cargo run -- toggle 2        # toggle reminder
cargo run -- rm 1
cargo run -- clear --done    # delete finished tasks (or `clear` for all)
```

Tasks are saved to `./tasks.tsv` by default; set `TASK_TRACKER_FILE` to use another path.
The file is plain tab-separated text (`id, reminder, done, day, text`) and is written
atomically; files from older versions without the `done` column still load.
Days written as `YYYY-MM-DD` or `YYYY-MM-DD HH:MM` are sorted by date; any other text is kept as-is.

## Test

```sh
cargo test
cargo clippy -- -D warnings
```
