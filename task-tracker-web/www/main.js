import init, { Tracker } from "./pkg/task_tracker_web.js";
import { createSync } from "./sync.js";

const STORAGE_KEY = "task-tracker.tsv";
const PREFS_KEY = "task-tracker.prefs";
const NOTIFIED_KEY = "task-tracker.notified";
const UNDO_MS = 8000;
const $ = (id) => document.getElementById(id);

function readStorage(key) {
  try { return localStorage.getItem(key); } catch { return null; }
}
function writeStorage(key, value) {
  try { localStorage.setItem(key, value); } catch { /* storage unavailable: keep in memory */ }
}
function readJson(key, fallback) {
  try { return JSON.parse(readStorage(key)) ?? fallback; } catch { return fallback; }
}

// ---------- dates ----------
// Dates are stored as "YYYY-MM-DD" or "YYYY-MM-DD HH:MM" so they stay readable
// (and sortable) in the CLI's tasks.tsv. Anything else is shown as free text.
const DAY_RE = /^(\d{4})-(\d{2})-(\d{2})(?: (\d{2}):(\d{2}))?$/;
const DATE_ONLY_NOTIFY_HOUR = 9; // reminders for date-only tasks fire at 9:00
const REPEAT_LABEL = { daily: "매일", weekly: "매주", monthly: "매월" };

function pad(n) { return String(n).padStart(2, "0"); }
function isoDate(d) { return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`; }
function daysFromNow(n) { const d = new Date(); d.setDate(d.getDate() + n); return isoDate(d); }

function parseDay(day) {
  const m = DAY_RE.exec(day);
  if (!m) return null;
  const [, y, mo, d, h, mi] = m;
  const hasTime = h !== undefined;
  const at = (hh, mm) => new Date(+y, +mo - 1, +d, hh, mm);
  const date = hasTime ? at(+h, +mi) : at(23, 59);
  // Reject dates JS would silently roll over (e.g. 2026-02-31 -> March 3).
  if (Number.isNaN(date.getTime()) || date.getMonth() !== +mo - 1 || date.getDate() !== +d) return null;
  return {
    date,
    hasTime,
    datePart: `${y}-${mo}-${d}`,
    timePart: hasTime ? `${h}:${mi}` : "",
    notifyAt: hasTime ? date : at(DATE_ONLY_NOTIFY_HOUR, 0),
  };
}

function formatDay(day) {
  const parsed = parseDay(day);
  if (!parsed) return { text: day, overdue: false };
  const { date, hasTime } = parsed;
  const weekday = "일월화수목금토"[date.getDay()];
  let text = `${date.getMonth() + 1}월 ${date.getDate()}일 (${weekday})`;
  if (date.getFullYear() !== new Date().getFullYear()) text = `${date.getFullYear()}년 ${text}`;
  if (hasTime) {
    text += " " + new Intl.DateTimeFormat("ko-KR", { hour: "numeric", minute: "2-digit" }).format(date);
  }
  return { text, overdue: date < new Date() };
}

function truncate(s, n) { return s.length > n ? s.slice(0, n - 1) + "…" : s; }

// ---------- state ----------
const SAMPLE = [
  ["병원 예약", `${daysFromNow(1)} 14:30`, true, ""],
  ["학교 상담", `${daysFromNow(2)} 13:30`, true, ""],
  ["장보기", daysFromNow(1), false, "weekly"],
];

await init();

let tracker;
const saved = readStorage(STORAGE_KEY);
try {
  tracker = saved != null ? Tracker.load(saved) : new Tracker();
} catch (e) {
  console.warn("저장된 데이터를 읽지 못해 새로 시작합니다:", e);
  tracker = new Tracker();
}
if (saved == null) SAMPLE.forEach(([t, d, r, rep]) => tracker.add(t, d, r, rep));

const prefs = { sort: "date", onlyReminders: false, hideDone: false, ...readJson(PREFS_KEY, {}) };
$("sort").value = prefs.sort;
$("only-reminders").checked = prefs.onlyReminders;
$("hide-done").checked = prefs.hideDone;

/** Task being edited: { id, legacyDay } or null when adding. */
let editing = null;
/** Current search text (not remembered between visits). */
let query = "";

function allTasks() {
  return JSON.parse(tracker.tasksJson(prefs.sort === "date", ""));
}

function persist() {
  writeStorage(STORAGE_KEY, tracker.save());
  render();
  checkDue();
  sync.schedule();
}

/** Replace the whole list (used by GitHub sync). Throws if `tsv` is not valid. */
function replaceList(tsv) {
  const next = Tracker.load(tsv);
  tracker.free();
  tracker = next;
  if (editing && !allTasks().some((t) => t.id === editing.id)) closeForm();
  writeStorage(STORAGE_KEY, tracker.save());
  render();
  checkDue();
}

// ---------- undo ----------
// Before a destructive change we keep the saved TSV; "되돌리기" (or Ctrl+Z) restores it.
// Only the latest change can be undone, and only for UNDO_MS.
let undoState = null; // { snapshot, el, timer }

function clearUndo() {
  if (!undoState) return;
  clearTimeout(undoState.timer);
  undoState.el.remove();
  undoState = null;
}

function offerUndo(message, snapshot) {
  clearUndo();
  const el = toast(message, { label: "되돌리기", onClick: undo, ms: UNDO_MS });
  undoState = { snapshot, el, timer: setTimeout(clearUndo, UNDO_MS) };
}

function undo() {
  if (!undoState) return;
  const { snapshot } = undoState;
  clearUndo();
  try {
    const next = Tracker.load(snapshot);
    closeForm();
    tracker.free();
    tracker = next;
    persist();
    toast("되돌렸어요", { ms: 3000 });
  } catch (err) {
    toast("되돌리지 못했어요: " + (err.message ?? err), { ms: 5000 });
  }
}

/** Run a change, save it, and offer to undo it. */
function mutate(message, change) {
  const snapshot = tracker.save();
  change();
  persist();
  offerUndo(message, snapshot);
}

document.addEventListener("keydown", (e) => {
  if (!undoState || e.shiftKey || !(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== "z") return;
  if (e.target.closest?.("input, textarea, select")) return; // keep native text undo
  e.preventDefault();
  undo();
});

// ---------- rendering ----------
function render() {
  const all = allTasks();
  const matched = JSON.parse(tracker.tasksJson(prefs.sort === "date", query));
  const shown = matched
    .filter((t) => !prefs.onlyReminders || t.reminder)
    .filter((t) => !prefs.hideDone || !t.done);
  $("tasks").replaceChildren(...shown.map(taskItem));
  $("empty").hidden = shown.length > 0;
  $("empty").textContent = query.trim()
    ? `"${query.trim()}"에 맞는 할 일이 없습니다`
    : "표시할 할 일이 없습니다";
  const done = all.filter((t) => t.done).length;
  $("count").textContent = query.trim()
    ? `검색 ${shown.length}개 · 전체 ${all.length}개`
    : `${all.length}개 · 완료 ${done}개`;
  $("clear-done").disabled = done === 0;
}

function taskItem(task) {
  const li = document.createElement("li");
  li.className = "task" + (task.reminder ? " reminder" : "") + (task.done ? " done" : "");
  li.title = "더블클릭하면 알림 켜기/끄기";
  li.addEventListener("dblclick", (e) => {
    if (e.target.closest("button, input")) return;
    tracker.toggle(task.id);
    persist();
  });

  const check = document.createElement("input");
  check.type = "checkbox";
  check.className = "done-check";
  check.checked = task.done;
  check.setAttribute("aria-label", task.done ? `${task.text} 완료 취소` : `${task.text} 완료`);
  check.addEventListener("change", () => completeTask(task));

  const body = document.createElement("div");
  body.className = "task-body";
  const h = document.createElement("h3");
  h.textContent = task.text;
  body.append(h);
  if (task.day) {
    const { text, overdue } = formatDay(task.day);
    const p = document.createElement("p");
    const late = overdue && !task.done;
    p.textContent = late ? `${text} · 지남` : text;
    if (late) p.className = "overdue";
    p.title = task.day;
    if (task.repeat) {
      const badge = document.createElement("span");
      badge.className = "repeat-badge";
      badge.textContent = `🔁 ${REPEAT_LABEL[task.repeat] ?? task.repeat}`;
      p.append(" ", badge);
    }
    body.append(p);
  }

  const actions = document.createElement("div");
  actions.className = "task-actions";
  const edit = button("icon", "✎", "수정");
  edit.addEventListener("click", () => startEdit(task));
  const bell = button("icon bell", task.reminder ? "🔔" : "🔕", task.reminder ? "알림 끄기" : "알림 켜기");
  bell.setAttribute("aria-pressed", String(task.reminder));
  bell.addEventListener("click", () => { tracker.toggle(task.id); persist(); });
  const del = button("icon del", "✕", "삭제");
  del.addEventListener("click", () => {
    if (editing?.id === task.id) closeForm();
    mutate(`"${truncate(task.text, 20)}" 삭제했어요`, () => tracker.remove(task.id));
  });
  actions.append(edit, bell, del);

  li.append(check, body, actions);
  return li;
}

/** Checkbox handler: complete / reopen; a repeating task moves to its next day instead. */
function completeTask(task) {
  const snapshot = tracker.save();
  tracker.toggleDone(task.id, isoDate(new Date()));
  persist();
  const now = allTasks().find((t) => t.id === task.id);
  if (now && !task.done && !now.done && now.day !== task.day) {
    offerUndo(`🔁 다음 일정: ${formatDay(now.day).text}`, snapshot);
  }
}

function button(cls, text, label) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = cls;
  b.textContent = text;
  b.setAttribute("aria-label", label);
  b.title = label;
  return b;
}

// ---------- add / edit form ----------
function openForm() {
  $("add-form").hidden = false;
  const btn = $("toggle-form");
  btn.textContent = "닫기";
  btn.classList.add("secondary");
  btn.setAttribute("aria-expanded", "true");
  $("text").focus();
}

function closeForm() {
  editing = null;
  $("add-form").reset();
  $("add-form").hidden = true;
  $("form-error").textContent = "";
  $("legacy-day").hidden = true;
  $("form-title").textContent = "새 할 일";
  $("submit").textContent = "저장";
  $("cancel-edit").hidden = true;
  const btn = $("toggle-form");
  btn.textContent = "추가";
  btn.classList.remove("secondary");
  btn.setAttribute("aria-expanded", "false");
}

function startEdit(task) {
  closeForm();
  const parsed = parseDay(task.day);
  editing = { id: task.id, legacyDay: task.day && !parsed ? task.day : "" };
  $("text").value = task.text;
  $("date").value = parsed?.datePart ?? "";
  $("time").value = parsed?.timePart ?? "";
  $("repeat").value = task.repeat ?? "";
  $("reminder").checked = task.reminder;
  if (editing.legacyDay) {
    $("legacy-day").textContent = `기존 날짜: "${editing.legacyDay}" — 새로 선택하면 바뀝니다`;
    $("legacy-day").hidden = false;
  }
  $("form-title").textContent = "할 일 수정";
  $("submit").textContent = "수정 저장";
  $("cancel-edit").hidden = false;
  openForm();
  $("add-form").scrollIntoView({ block: "nearest" });
}

$("toggle-form").addEventListener("click", () => {
  if ($("add-form").hidden) openForm(); else closeForm();
});
$("cancel-edit").addEventListener("click", closeForm);

$("add-form").addEventListener("submit", (e) => {
  e.preventDefault();
  const date = $("date").value;   // "YYYY-MM-DD" or ""
  const time = $("time").value;   // "HH:MM" or ""
  const repeat = $("repeat").value;
  if (time && !date) {
    $("form-error").textContent = "시간을 넣으려면 날짜도 선택하세요";
    $("date").focus();
    return;
  }
  if (repeat && !date) {
    $("form-error").textContent = "반복하려면 날짜를 선택하세요";
    $("date").focus();
    return;
  }
  let day = date ? (time ? `${date} ${time}` : date) : "";
  if (!day && editing?.legacyDay) day = editing.legacyDay; // keep a free-text day unless replaced
  const reminder = $("reminder").checked;
  try {
    if (editing) {
      tracker.update(editing.id, $("text").value, day, repeat);
      const current = allTasks().find((t) => t.id === editing.id);
      if (current && current.reminder !== reminder) tracker.toggle(editing.id);
    } else {
      tracker.add($("text").value, day, reminder, repeat);
    }
  } catch (err) {
    $("form-error").textContent = err.message ?? String(err);
    $("text").focus();
    return;
  }
  if (editing) closeForm();
  else { $("form-error").textContent = ""; e.target.reset(); $("text").focus(); }
  persist();
});

// ---------- search / filters ----------
$("search").addEventListener("input", (e) => {
  query = e.target.value;
  render();
});

function savePrefs() {
  prefs.sort = $("sort").value;
  prefs.onlyReminders = $("only-reminders").checked;
  prefs.hideDone = $("hide-done").checked;
  writeStorage(PREFS_KEY, JSON.stringify(prefs));
  render();
}
["sort", "only-reminders", "hide-done"].forEach((id) => $(id).addEventListener("change", savePrefs));

// ---------- bulk actions / import / export ----------
$("clear").addEventListener("click", () => {
  if (!confirm("모든 할 일을 삭제할까요?")) return;
  closeForm();
  mutate("모든 할 일을 삭제했어요", () => tracker.clear());
});

$("clear-done").addEventListener("click", () => {
  const snapshot = tracker.save();
  const n = tracker.clearDone();
  persist();
  if (n > 0) offerUndo(`완료한 할 일 ${n}개를 삭제했어요`, snapshot);
  else toast("삭제할 완료 항목이 없어요", { ms: 3000 });
});

$("export").addEventListener("click", () => {
  const blob = new Blob([tracker.save()], { type: "text/tab-separated-values" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = "tasks.tsv";
  a.click();
  URL.revokeObjectURL(a.href);
});

$("import").addEventListener("change", async (e) => {
  const file = e.target.files?.[0];
  if (!file) return;
  try {
    const next = Tracker.load(await file.text());
    const snapshot = tracker.save();
    closeForm();
    tracker.free();
    tracker = next;
    persist();
    offerUndo("파일을 가져와 목록을 바꿨어요", snapshot);
  } catch (err) {
    alert("파일을 읽지 못했습니다: " + (err.message ?? err));
  }
  e.target.value = "";
});

// ---------- reminders / browser notifications ----------
// Notifications fire only while this page is open (there is no server to push them).
const notified = new Set(readJson(NOTIFIED_KEY, []));
const supportsNotifications = "Notification" in window;

function updateNotifyUi() {
  const btn = $("notify-btn");
  const status = $("notify-status");
  if (!supportsNotifications) {
    btn.hidden = true;
    status.textContent = "이 브라우저는 알림을 지원하지 않아요. 알림 시간이 되면 이 화면에 표시합니다.";
    return;
  }
  const perm = Notification.permission;
  btn.hidden = perm !== "default";
  status.textContent = {
    granted: "🔔 브라우저 알림 켜짐 — 이 페이지가 열려 있으면 알림 설정한 일정 시간에 알려드려요.",
    denied: "브라우저 설정에서 이 사이트의 알림이 차단돼 있어요. 알림 시간이 되면 이 화면에만 표시합니다.",
    default: "알림 설정한 일정 시간에 알림을 받으려면 켜 주세요. (날짜만 있으면 그날 오전 9시)",
  }[perm];
}

$("notify-btn").addEventListener("click", async () => {
  try { await Notification.requestPermission(); } catch { /* older Safari: callback form */ }
  updateNotifyUi();
  if (Notification.permission === "granted") {
    new Notification("Task Tracker", { body: "알림이 켜졌어요 🔔" });
    checkDue();
  }
});

function checkDue() {
  const now = new Date();
  for (const t of allTasks()) {
    if (!t.reminder || t.done) continue;
    const parsed = parseDay(t.day);
    if (!parsed) continue;
    const key = `${t.id}|${t.day}`; // a repeating task's next day is a new key, so it notifies again
    const lateBy = now - parsed.notifyAt;
    // Due now, or became due within the last day while the page was closed.
    if (lateBy < 0 || lateBy > 24 * 60 * 60 * 1000 || notified.has(key)) continue;
    notified.add(key);
    const when = formatDay(t.day).text;
    if (supportsNotifications && Notification.permission === "granted") {
      try { new Notification(`⏰ ${t.text}`, { body: when, tag: key }); } catch { /* mobile needs a service worker */ }
    }
    toast(`⏰ ${t.text} — ${when}`);
  }
  writeStorage(NOTIFIED_KEY, JSON.stringify([...notified].slice(-200)));
}

/** Show a message at the bottom of the page. Returns the element. */
function toast(message, { label, onClick, ms = 10000 } = {}) {
  const el = document.createElement("div");
  el.className = "toast";
  const text = document.createElement("span");
  text.textContent = message;
  const actions = document.createElement("div");
  actions.className = "toast-actions";
  if (label) {
    const act = document.createElement("button");
    act.type = "button";
    act.className = "toast-action";
    act.textContent = label;
    act.addEventListener("click", onClick);
    actions.append(act);
  }
  const close = button("icon", "✕", "닫기");
  close.addEventListener("click", () => el.remove());
  actions.append(close);
  el.append(text, actions);
  $("toasts").append(el);
  setTimeout(() => el.remove(), ms);
  return el;
}

// ---------- GitHub sync ----------
function timeText(d) {
  return new Intl.DateTimeFormat("ko-KR", { hour: "numeric", minute: "2-digit" }).format(d);
}

function onSyncStatus({ state, message, at, gistUrl, connected }) {
  $("sync-setup").hidden = connected;
  $("sync-connected").hidden = !connected;
  $("sync-conflict").hidden = state !== "conflict";
  $("sync-open").hidden = !gistUrl;
  if (gistUrl) $("sync-open").href = gistUrl;
  const last = at ? ` (마지막 저장 ${timeText(at)})` : "";
  $("sync-status").textContent = {
    off: "연결하면 할 일이 내 GitHub 계정에 자동 저장되고, 다른 기기에서도 같은 목록을 볼 수 있어요.",
    syncing: "GitHub과 맞추는 중…",
    ok: `✓ GitHub에 저장됨${at ? ` · ${timeText(at)}` : ""}`,
    conflict: "⚠️ 아래에서 선택하기 전까지는 GitHub에 저장하지 않습니다.",
    auth: `⚠️ ${message}`,
    offline: `⚠️ ${message}${last}`,
    error: `⚠️ ${message}${last}`,
  }[state];
  $("sync-status").classList.toggle("error-text", ["auth", "offline", "error", "conflict"].includes(state));
}

const sync = createSync({
  getTsv: () => tracker.save(),
  applyTsv: replaceList,
  onStatus: onSyncStatus,
  onPulled: () => toast("다른 기기에서 바뀐 내용을 불러왔어요", { ms: 4000 }),
});

$("sync-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const token = $("sync-token").value.trim();
  if (!token) {
    $("sync-error").textContent = "토큰을 붙여 넣어 주세요";
    return;
  }
  $("sync-error").textContent = "";
  $("sync-connect").disabled = true;
  $("sync-connect").textContent = "확인 중…";
  try {
    await sync.connect(token);
    $("sync-token").value = "";
  } catch (err) {
    $("sync-error").textContent =
      err.status === 401 ? "토큰이 올바르지 않아요. 다시 복사해서 붙여 넣어 주세요."
      : err.status === 403 || err.status === 404 ? "이 토큰에는 gist 권한이 없어요. gist를 체크해서 다시 만들어 주세요."
      : err.status ? `GitHub 오류 (${err.status}). 잠시 후 다시 시도해 주세요.`
      : "GitHub에 연결하지 못했어요. 인터넷 연결을 확인해 주세요.";
  } finally {
    $("sync-connect").disabled = false;
    $("sync-connect").textContent = "연결";
  }
});

$("sync-now").addEventListener("click", () => sync.syncNow());
$("sync-disconnect").addEventListener("click", () => {
  if (!confirm("이 브라우저에서 GitHub 연결을 끊을까요?\n할 일은 이 브라우저와 GitHub에 그대로 남습니다.")) return;
  sync.disconnect();
});
$("sync-use-remote").addEventListener("click", async () => {
  const snapshot = tracker.save();
  try {
    await sync.resolve("remote");
    offerUndo("GitHub의 목록으로 바꿨어요", snapshot);
  } catch (err) {
    toast("GitHub의 목록을 읽지 못했어요: " + (err.message ?? err), { ms: 6000 });
  }
});
$("sync-use-local").addEventListener("click", () => sync.resolve("local"));

updateNotifyUi();
render();
checkDue();
sync.start();
setInterval(() => { checkDue(); render(); }, 30 * 1000);
