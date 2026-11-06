# task-tracker-web (Rust → WebAssembly)

The browser version of the Rust task tracker. All task logic (add, delete, toggle reminder,
the TSV save format) is the same Rust code as the CLI in `../task-tracker-cli`, compiled to
WebAssembly; the page itself is plain HTML/CSS/JS in `www/`.

Tasks are kept in the browser's localStorage. **Export / import `tasks.tsv`** to move tasks
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
