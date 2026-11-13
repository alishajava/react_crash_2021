// Sync the task list with a secret GitHub Gist, using the user's own token.
//
// The gist holds one file (task-tracker.json) wrapping the same TSV the app
// stores locally. Every sync is a 3-way comparison between:
//   local  — what this browser has now
//   remote — what the gist has now
//   base   — what both sides had after the last successful sync
// so changes made on another device are pulled, local changes are pushed, and
// if both changed we stop and ask instead of overwriting either side.

const API = "https://api.github.com";
const FILE = "task-tracker.json";
const CONFIG_KEY = "task-tracker.sync";
const DEBOUNCE_MS = 1500;
const POLL_MS = 60 * 1000;

class HttpError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

function readConfig() {
  try { return JSON.parse(localStorage.getItem(CONFIG_KEY)) ?? {}; } catch { return {}; }
}
function writeConfig(cfg) {
  try { localStorage.setItem(CONFIG_KEY, JSON.stringify(cfg)); } catch { /* keep in memory */ }
}

/**
 * @param {object} hooks
 * @param {() => string} hooks.getTsv            current local list
 * @param {(tsv: string) => void} hooks.applyTsv replace the local list (throws if invalid)
 * @param {(status: object) => void} hooks.onStatus  { state, message?, at?, gistUrl? }
 * @param {(tsv: string) => void} hooks.onPulled called after remote changes were applied automatically
 */
export function createSync({ getTsv, applyTsv, onStatus, onPulled }) {
  let cfg = readConfig(); // { token, gistId, gistUrl, base }
  let busy = false;
  let again = false;
  let timer = null;
  let pendingRemote = null; // remote TSV waiting for the user's choice
  let lastOk = null;

  const connected = () => Boolean(cfg.token);

  function status(state, message) {
    onStatus({ state, message, at: lastOk, gistUrl: cfg.gistUrl, connected: connected() });
  }

  async function api(path, options = {}, token = cfg.token) {
    const res = await fetch(API + path, {
      ...options,
      cache: "no-store", // gists are served with max-age; we need the current content
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: `Bearer ${token}`,
        "X-GitHub-Api-Version": "2022-11-28",
        ...(options.body ? { "Content-Type": "application/json" } : {}),
      },
    });
    if (!res.ok) throw new HttpError(res.status, `GitHub ${res.status}`);
    return res.json();
  }

  function wrap(tsv) {
    return JSON.stringify({ app: "task-tracker", version: 1, updatedAt: new Date().toISOString(), tsv }, null, 2);
  }

  /** The gist's TSV, or null if the gist has no usable file yet. */
  async function readRemote() {
    const gist = await api(`/gists/${cfg.gistId}`);
    cfg.gistUrl = gist.html_url ?? cfg.gistUrl;
    const file = gist.files?.[FILE];
    if (!file) return null;
    let content = file.content;
    if (file.truncated && file.raw_url) content = await (await fetch(file.raw_url, { cache: "no-store" })).text();
    const data = JSON.parse(content);
    if (typeof data?.tsv !== "string") throw new Error("bad gist content");
    return data.tsv;
  }

  async function push(tsv) {
    await api(`/gists/${cfg.gistId}`, {
      method: "PATCH",
      body: JSON.stringify({ files: { [FILE]: { content: wrap(tsv) } } }),
    });
  }

  /** Find this account's task-tracker gist, or create one from the local list. */
  async function findOrCreateGist() {
    for (let page = 1; page <= 10; page++) {
      const gists = await api(`/gists?per_page=100&page=${page}`);
      const found = gists.find((g) => g.files && FILE in g.files);
      if (found) {
        cfg.gistId = found.id;
        cfg.gistUrl = found.html_url;
        return;
      }
      if (gists.length < 100) break;
    }
    const tsv = getTsv();
    const created = await api("/gists", {
      method: "POST",
      body: JSON.stringify({
        description: "Task Tracker — 자동 저장 (지우면 동기화가 끊깁니다)",
        public: false,
        files: { [FILE]: { content: wrap(tsv) } },
      }),
    });
    cfg.gistId = created.id;
    cfg.gistUrl = created.html_url;
    cfg.base = tsv;
  }

  async function syncOnce() {
    if (!cfg.gistId) await findOrCreateGist();
    let remote;
    try {
      remote = await readRemote();
    } catch (e) {
      if (e instanceof HttpError && e.status === 404) {
        // The gist was deleted on GitHub: find the one another device recreated,
        // or create a new one. `base` is kept so the 3-way comparison still works.
        cfg.gistId = cfg.gistUrl = undefined;
        await findOrCreateGist();
        remote = await readRemote();
      } else {
        throw e;
      }
    }
    const local = getTsv();

    if (remote === null || remote === local) {
      if (remote === null) await push(local);
      cfg.base = local;
    } else if (cfg.base === local) {
      applyTsv(remote); // only the other side changed
      cfg.base = remote;
      onPulled(remote);
    } else if (cfg.base === remote) {
      await push(local); // only this side changed
      cfg.base = local;
    } else {
      pendingRemote = remote; // both changed (or first connect with different lists)
      writeConfig(cfg);
      return "conflict";
    }
    pendingRemote = null;
    writeConfig(cfg);
    return "ok";
  }

  async function sync() {
    if (!connected()) return;
    if (busy) { again = true; return; }
    busy = true;
    status("syncing");
    try {
      const result = await syncOnce();
      if (result === "conflict") {
        status("conflict");
      } else {
        lastOk = new Date();
        status("ok");
      }
    } catch (e) {
      if (e instanceof HttpError && e.status === 401) {
        status("auth", "토큰이 만료됐거나 잘못됐어요. 연결을 끊고 새 토큰으로 다시 연결해 주세요.");
      } else if (e instanceof HttpError && (e.status === 403 || e.status === 429)) {
        status("error", "GitHub이 요청을 거절했어요 (권한 또는 사용량 제한). 토큰에 gist 권한이 있는지 확인해 주세요.");
      } else if (e instanceof HttpError) {
        status("error", `GitHub 오류 (${e.status}). 잠시 후 다시 시도합니다.`);
      } else if (e instanceof TypeError) {
        status("offline", "인터넷에 연결되지 않았어요. 연결되면 자동으로 저장합니다.");
      } else {
        status("error", "GitHub에 저장된 데이터를 읽지 못했어요. 이 기기의 목록은 그대로입니다.");
      }
    } finally {
      busy = false;
      if (again) { again = false; schedule(); }
    }
  }

  /** Call after every local change; pushes shortly afterwards. */
  function schedule() {
    if (!connected()) return;
    clearTimeout(timer);
    timer = setTimeout(sync, DEBOUNCE_MS);
  }

  /** Check the token, remember it, and run the first sync. Throws HttpError on a bad token. */
  async function connect(token) {
    token = token.trim();
    await api("/gists?per_page=1", {}, token); // 401 if invalid, 403/404 without gist permission
    cfg = { token };
    writeConfig(cfg);
    await sync();
  }

  /** Forget the token on this browser. The gist stays on GitHub. */
  function disconnect() {
    clearTimeout(timer);
    cfg = {};
    pendingRemote = null;
    lastOk = null;
    writeConfig(cfg);
    status("off");
  }

  /** Settle a conflict: "remote" replaces this device's list, "local" overwrites GitHub. */
  async function resolve(choice) {
    if (pendingRemote === null) return;
    if (choice === "remote") {
      applyTsv(pendingRemote);
      cfg.base = pendingRemote;
      pendingRemote = null;
      writeConfig(cfg);
      lastOk = new Date();
      status("ok");
    } else {
      cfg.base = pendingRemote; // "only this side changed" -> next sync pushes local
      pendingRemote = null;
      writeConfig(cfg);
      await sync();
    }
  }

  function start() {
    status(connected() ? "syncing" : "off");
    if (!connected()) return;
    sync();
  }

  window.addEventListener("online", () => sync());
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") sync();
  });
  setInterval(() => {
    if (document.visibilityState === "visible") sync();
  }, POLL_MS);

  return { start, schedule, syncNow: sync, connect, disconnect, resolve, connected };
}
