# task-tracker (Rust CLI)

A small command-line companion to the React Task Tracker in this repo, written in
Rust with no external dependencies. Each task has text, a day/time, a reminder flag and a done flag.

## Build & run

```sh
cd task-tracker-cli
cargo run -- add "Doctors appointment" --day "2026-02-05 14:30" --reminder
cargo run -- add "Meeting at school" -d 2026-02-06
cargo run -- add 약 먹기 -d "2026-02-05 08:00" --repeat daily   # daily | weekly | monthly
cargo run -- list            # open tasks first, sorted by date
cargo run -- list 병원        # search text and day (all words must match; also: search, find)
cargo run -- list --by-id    # in the order added
cargo run -- list -r -o      # only reminders, hide finished
cargo run -- done 2          # complete (repeating tasks move to their next day) / reopen
cargo run -- edit 1 --repeat none
cargo run -- edit 1 Dentist --day "2026-02-07 09:00"
cargo run -- toggle 2        # toggle reminder
cargo run -- rm 1
cargo run -- clear --done    # delete finished tasks (or `clear` for all)
```

Tasks are saved to `./tasks.tsv` by default; set `TASK_TRACKER_FILE` to use another path.
The file is plain tab-separated text (`id, reminder, done, repeat, day, text`) and is written
atomically; files from older versions (without `done` and/or `repeat`) still load.
Days written as `YYYY-MM-DD` or `YYYY-MM-DD HH:MM` are sorted by date; any other text is kept as-is.

## Test

```sh
cargo test
cargo clippy -- -D warnings
```

## Repeating tasks

`--repeat daily|weekly|monthly` needs a `YYYY-MM-DD` or `YYYY-MM-DD HH:MM` day. Completing the
task with `done` does not finish it: its day moves forward one step and keeps moving until it is
after today, so an overdue task doesn't come back already overdue. Monthly tasks keep their
day-of-month where they can (Jan 31 → Feb 28 → Mar 31). A repeating task with a free-text day
(or none) is completed normally.

"Today" is the current UTC date; set `TASK_TRACKER_TODAY=YYYY-MM-DD` to override it.
