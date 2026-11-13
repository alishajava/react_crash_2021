# task-tracker-web (Rust → WebAssembly)

The browser version of the Rust task tracker. All task logic (add, delete, toggle reminder,
the TSV save format) is the same Rust code as the CLI in `../task-tracker-cli`, compiled to
WebAssembly; the page itself is plain HTML/CSS/JS in `www/`.

Dates are picked with a calendar and stored as `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`
(free-text days from the CLI still display as-is). Features: repeating tasks (daily / weekly / monthly: completing one moves it to its next day),
search by text or date, undo for deletes / clear / import / completing a repeating task
(the "되돌리기" button or Ctrl+Z, for 8 seconds), mark tasks done, edit them, sort by date or by order added, filter reminders /
hide finished, and browser notifications for reminders (while the page is open; date-only
tasks notify at 9:00). Tasks are kept in the browser's localStorage. **Export / import `tasks.tsv`** to move tasks
between the web page and the CLI.

## Build locally

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked   # must match Cargo.lock
./build.sh
cd dist && python3 -m http.server 8000   # open http://localhost:8000
```

## Deploy

`.github/workflows/pages.yml` tests, builds and deploys the site to GitHub Pages on push.
One-time setup: in the repository's **Settings → Pages**, set **Source** to **GitHub Actions**.

## Saving to GitHub (optional)

The "☁️ GitHub에 자동 저장" section syncs the list with a secret Gist in the user's own GitHub
account, so it survives clearing browser data and is shared between devices. It needs a personal
access token with only the `gist` scope; the token is kept in that browser's localStorage and is
sent only to `api.github.com`.

`www/sync.js` does a 3-way comparison (this device, the gist, and what both had after the last
sync): changes from another device are pulled, local changes are pushed about 1.5 s after the
last edit, and if both sides changed it asks which list to keep instead of overwriting either.
It also syncs on page load, when the tab becomes visible, every minute, and when coming back online.
