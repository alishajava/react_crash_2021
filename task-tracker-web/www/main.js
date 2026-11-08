import init, { Tracker } from "./pkg/task_tracker_web.js";

const STORAGE_KEY = "task-tracker.tsv";
const $ = (id) => document.getElementById(id);

function readStorage() {
  try { return localStorage.getItem(STORAGE_KEY); } catch { return null; }
}
function writeStorage(value) {
  try { localStorage.setItem(STORAGE_KEY, value); } catch { /* storage unavailable: keep in memory */ }
}

// Dates are stored as "YYYY-MM-DD" or "YYYY-MM-DD HH:MM" so they stay readable
// (and sortable) in the CLI's tasks.tsv. Anything else is shown as free text.
const DAY_RE = /^(\d{4})-(\d{2})-(\d{2})(?: (\d{2}):(\d{2}))?$/;

function pad(n) { return String(n).padStart(2, "0"); }
function isoDate(d) { return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`; }
function daysFromNow(n) { const d = new Date(); d.setDate(d.getDate() + n); return isoDate(d); }

function parseDay(day) {
  const m = DAY_RE.exec(day);
  if (!m) return null;
  const [, y, mo, d, h, mi] = m;
  const date = new Date(+y, +mo - 1, +d, h ? +h : 23, mi ? +mi : 59);
  return Number.isNaN(date.getTime()) ? null : { date, hasTime: h !== undefined };
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

const SAMPLE = [
  ["병원 예약", `${daysFromNow(1)} 14:30`, true],
  ["학교 상담", `${daysFromNow(2)} 13:30`, true],
  ["장보기", daysFromNow(1), false],
];

await init();

let tracker;
const saved = readStorage();
try {
  tracker = saved != null ? Tracker.load(saved) : new Tracker();
} catch (e) {
  console.warn("저장된 데이터를 읽지 못해 새로 시작합니다:", e);
  tracker = new Tracker();
}
if (saved == null) SAMPLE.forEach(([t, d, r]) => tracker.add(t, d, r));

function persist() {
  writeStorage(tracker.save());
  render();
}

function render() {
  const all = JSON.parse(tracker.tasksJson());
  const onlyReminders = $("only-reminders").checked;
  const shown = onlyReminders ? all.filter((t) => t.reminder) : all;
  const list = $("tasks");
  list.replaceChildren(...shown.map(taskItem));
  $("empty").hidden = shown.length > 0;
  $("count").textContent = `${all.length}개 · 알림 ${all.filter((t) => t.reminder).length}개`;
}

function taskItem(task) {
  const li = document.createElement("li");
  li.className = "task" + (task.reminder ? " reminder" : "");
  li.title = "더블클릭하면 알림 켜기/끄기";
  li.addEventListener("dblclick", () => { tracker.toggle(task.id); persist(); });

  const body = document.createElement("div");
  const h = document.createElement("h3");
  h.textContent = task.text;
  body.append(h);
  if (task.day) {
    const { text, overdue } = formatDay(task.day);
    const p = document.createElement("p");
    p.textContent = overdue ? `${text} · 지남` : text;
    if (overdue) p.className = "overdue";
    p.title = task.day;
    body.append(p);
  }

  const actions = document.createElement("div");
  actions.className = "task-actions";
  const bell = button("icon bell", task.reminder ? "🔔" : "🔕", task.reminder ? "알림 끄기" : "알림 켜기");
  bell.setAttribute("aria-pressed", String(task.reminder));
  bell.addEventListener("click", () => { tracker.toggle(task.id); persist(); });
  const del = button("icon del", "✕", "삭제");
  del.addEventListener("click", () => { tracker.remove(task.id); persist(); });
  actions.append(bell, del);

  li.append(body, actions);
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

$("toggle-form").addEventListener("click", () => {
  const form = $("add-form");
  form.hidden = !form.hidden;
  const btn = $("toggle-form");
  btn.textContent = form.hidden ? "추가" : "닫기";
  btn.classList.toggle("secondary", !form.hidden);
  btn.setAttribute("aria-expanded", String(!form.hidden));
  if (!form.hidden) $("text").focus();
});

$("add-form").addEventListener("submit", (e) => {
  e.preventDefault();
  const date = $("date").value;   // "YYYY-MM-DD" or ""
  const time = $("time").value;   // "HH:MM" or ""
  if (time && !date) {
    $("form-error").textContent = "시간을 넣으려면 날짜도 선택하세요";
    $("date").focus();
    return;
  }
  const day = date ? (time ? `${date} ${time}` : date) : "";
  try {
    tracker.add($("text").value, day, $("reminder").checked);
  } catch (err) {
    $("form-error").textContent = err.message ?? String(err);
    $("text").focus();
    return;
  }
  $("form-error").textContent = "";
  e.target.reset();
  persist();
});

$("only-reminders").addEventListener("change", render);

$("clear").addEventListener("click", () => {
  if (confirm("모든 할 일을 삭제할까요?")) { tracker.clear(); persist(); }
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
    tracker.free();
    tracker = next;
    persist();
  } catch (err) {
    alert("파일을 읽지 못했습니다: " + (err.message ?? err));
  }
  e.target.value = "";
});

render();
