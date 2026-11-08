import init, { Tracker } from "./pkg/task_tracker_web.js";

const STORAGE_KEY = "task-tracker.tsv";
const PREFS_KEY = "task-tracker.prefs";
const NOTIFIED_KEY = "task-tracker.notified";
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
  if (Number.isNaN(date.getTime())) return null;
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

// ---------- state ----------
const SAMPLE = [
  ["병원 예약", `${daysFromNow(1)} 14:30`, true],
  ["학교 상담", `${daysFromNow(2)} 13:30`, true],
  ["장보기", daysFromNow(1), false],
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
if (saved == null) SAMPLE.forEach(([t, d, r]) => tracker.add(t, d, r));

const prefs = { sort: "date", onlyReminders: false, hideDone: false, ...readJson(PREFS_KEY, {}) };
$("sort").value = prefs.sort;
$("only-reminders").checked = prefs.onlyReminders;
$("hide-done").checked = prefs.hideDone;

/** Task being edited: { id, legacyDay } or null when adding. */
let editing = null;

function allTasks() {
  return JSON.parse(tracker.tasksJson(prefs.sort === "date"));
}

function persist() {
  writeStorage(STORAGE_KEY, tracker.save());
  render();
  checkDue();
}

// ---------- rendering ----------
function render() {
  const all = allTasks();
  const shown = all
    .filter((t) => !prefs.onlyReminders || t.reminder)
    .filter((t) => !prefs.hideDone || !t.done);
  $("tasks").replaceChildren(...shown.map(taskItem));
  $("empty").hidden = shown.length > 0;
  const done = all.filter((t) => t.done).length;
  $("count").textContent = `${all.length}개 · 완료 ${done}개`;
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
  check.addEventListener("change", () => { tracker.toggleDone(task.id); persist(); });

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
    tracker.remove(task.id);
    persist();
  });
  actions.append(edit, bell, del);

  li.append(check, body, actions);
  return li;
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
  if (time && !date) {
    $("form-error").textContent = "시간을 넣으려면 날짜도 선택하세요";
    $("date").focus();
    return;
  }
  let day = date ? (time ? `${date} ${time}` : date) : "";
  if (!day && editing?.legacyDay) day = editing.legacyDay; // keep a free-text day unless replaced
  const reminder = $("reminder").checked;
  try {
    if (editing) {
      tracker.update(editing.id, $("text").value, day);
      const current = allTasks().find((t) => t.id === editing.id);
      if (current && current.reminder !== reminder) tracker.toggle(editing.id);
    } else {
      tracker.add($("text").value, day, reminder);
    }
  } catch (err) {
    $("form-error").textContent = err.message ?? String(err);
    $("text").focus();
    return;
  }
  const wasEditing = editing;
  if (wasEditing) closeForm();
  else { $("form-error").textContent = ""; e.target.reset(); $("text").focus(); }
  persist();
});

// ---------- filters ----------
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
  if (confirm("모든 할 일을 삭제할까요?")) { closeForm(); tracker.clear(); persist(); }
});
$("clear-done").addEventListener("click", () => {
  const n = tracker.clearDone();
  toast(`완료한 할 일 ${n}개를 삭제했어요`);
  persist();
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
    closeForm();
    tracker.free();
    tracker = next;
    persist();
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
    const key = `${t.id}|${t.day}`;
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

function toast(message) {
  const el = document.createElement("div");
  el.className = "toast";
  el.textContent = message;
  const close = button("icon", "✕", "닫기");
  close.addEventListener("click", () => el.remove());
  el.append(close);
  $("toasts").append(el);
  setTimeout(() => el.remove(), 10000);
}

updateNotifyUi();
render();
checkDue();
setInterval(() => { checkDue(); render(); }, 30 * 1000);
