// Landing page behaviour. No framework: chat loop, view-as toggle, scroll reveals, live stats.
"use strict";

const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s) => document.querySelector(s);
const clamp01 = (v) => Math.min(1, Math.max(0, v));

function onVisible(el, cb, opts = {}) {
  const io = new IntersectionObserver((entries) => entries.forEach((e) => cb(e.isIntersecting, e)), opts);
  io.observe(el);
  return io;
}

/* ---------- chat: scripted loop while on screen ---------- */
const CHAT_SCRIPT = [ // [ms from start, action]: command, rule card, tap Allow, active, heads up, delivered, receipt
  [300, 0], [1300, 1], [2800, "press"], [3200, 2], [5000, 3], [6800, 4], [7800, 5],
];
const CHAT_HOLD_MS = 3200;

function startChat() {
  const body = $("#chat");
  if (!body) return;
  const steps = [...body.querySelectorAll("[data-step]")];
  const buy = body.querySelector("[data-press]");
  const reset = () => { steps.forEach((s) => s.classList.remove("on")); buy.classList.remove("pressed"); };
  const show = (a) => { if (a === "press") buy.classList.add("pressed"); else steps[a].classList.add("on"); };

  if (reduced) { steps.forEach((s) => s.classList.add("on")); return; }
  let timers = [];
  const stop = () => { timers.forEach(clearTimeout); timers = []; };
  const run = () => {
    stop(); reset();
    CHAT_SCRIPT.forEach(([ms, a]) => timers.push(setTimeout(() => show(a), ms)));
    const end = CHAT_SCRIPT[CHAT_SCRIPT.length - 1][0] + CHAT_HOLD_MS;
    timers.push(setTimeout(run, end));
  };
  onVisible(body, (v) => (v ? run() : stop()), { threshold: 0.3 });
}

/* ---------- view as: chain / Bitrefill / you ---------- */
const VIEW_NOTES = {
  chain: "The blockchain sees a private payment. No number, no amount, nothing that points to you.",
  bitrefill: "Bitrefill gets the number and amount it must deliver. It never sees your Zcash address or your Telegram account.",
  you: "With your viewing key, every field opens.",
};

function startViewAs() {
  const tabs = [...document.querySelectorAll(".seg [data-view]")];
  const fields = [...document.querySelectorAll("#receipt dd")];
  const note = $("#viewas-note");
  if (!tabs.length) return;

  const setField = (dd, view) => {
    const target = dd.dataset[view];
    const hidden = target === "hidden";
    dd.textContent = hidden ? "Hidden" : target;
    dd.classList.toggle("hidden", hidden);
  };
  const setView = (view, focus = false) => {
    tabs.forEach((t) => {
      const on = t.dataset.view === view;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      if (on && focus) t.focus();
    });
    fields.forEach((dd) => setField(dd, view));
    note.textContent = VIEW_NOTES[view];
  };
  tabs.forEach((t, i) => {
    t.addEventListener("click", () => setView(t.dataset.view));
    t.addEventListener("keydown", (e) => {
      const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (d) { e.preventDefault(); setView(tabs[(i + d + tabs.length) % tabs.length].dataset.view, true); }
    });
  });
  // start encrypted, decode to "You" the first time the receipt scrolls into view
  setView("chain");
  let done = false;
  const io = onVisible($(".receipt"), (v) => {
    if (v && !done) { done = true; io.disconnect(); setTimeout(() => setView("you"), 700); }
  }, { threshold: 0.6 });
}

/* ---------- reveals and tagline ---------- */
function startReveals() {
  const els = document.querySelectorAll(".reveal");
  const io = new IntersectionObserver((entries) => entries.forEach((e) => {
    if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
  }), { threshold: 0.12 });
  els.forEach((el) => io.observe(el));
}

function startTagline() {
  const el = $("#tagline");
  if (!el) return;
  const words = el.textContent.trim().split(/\s+/);
  el.setAttribute("aria-label", el.textContent.trim());
  el.replaceChildren();
  words.forEach((w, i) => {
    const s = document.createElement("span");
    s.className = "w"; s.textContent = w; s.setAttribute("aria-hidden", "true");
    el.append(s, i < words.length - 1 ? " " : "");
  });
  const spans = el.querySelectorAll(".w");
  if (reduced) { spans.forEach((s) => s.classList.add("lit")); return; }
  // a word lights when it crosses 60% of the viewport height, and stays lit
  const io = new IntersectionObserver((entries) => entries.forEach((e) => {
    if (e.isIntersecting) { e.target.classList.add("lit"); io.unobserve(e.target); }
  }), { rootMargin: "0px 0px -40% 0px" });
  spans.forEach((s) => io.observe(s));
}

/* ---------- copy chip ---------- */
function startCopy() {
  document.querySelectorAll("[data-copy]").forEach((b) => b.addEventListener("click", () => {
    const label = b.textContent;
    navigator.clipboard.writeText(b.dataset.copy)
      .then(() => { b.textContent = "Copied"; })
      .catch(() => { b.textContent = "Select and copy"; })
      .finally(() => setTimeout(() => { b.textContent = label; }, 1600));
  }));
}

/* ---------- live stats (FR-16): GET {STATS_URL}/stats.json every 10 s while visible ---------- */
const STATS_POLL_MS = 10_000;
const GLOBAL_CAP_INR = 200; // BR-3
// The server's shape (src/workers.rs stats_server). It sends no timestamp, so a 200 counts as live.
const STATS_KEYS = ["orders_delivered", "inr_spent_today", "receipts_sent"];

function startStats() {
  const panel = $("#stats");
  if (!panel) return;
  const base = (window.SSA_CONFIG && window.SSA_CONFIG.STATS_URL || "").replace(/\/$/, "");
  const cap = $("#stats-cap"), pill = $("#stats-pill");
  const offline = () => {
    panel.dataset.state = "offline";
    ["#s-delivered", "#s-inr", "#s-receipts"].forEach((s) => { $(s).textContent = "n/a"; });
    $("#s-bar").style.transform = "scaleX(0)";
    pill.hidden = true;
    cap.textContent = "No live link to the bot right now. It runs on our laptop at the event.";
  };
  if (!base) { offline(); return; }

  const valid = (j) => j && STATS_KEYS.every((k) => Number.isInteger(j[k]) && j[k] >= 0)
    && typeof j.paused === "boolean";
  const render = (j) => {
    const capped = j.inr_spent_today >= GLOBAL_CAP_INR;
    panel.dataset.state = capped ? "capped" : "live";
    $("#s-delivered").textContent = j.orders_delivered;
    $("#s-inr").textContent = `₹${j.inr_spent_today}`;
    $("#s-receipts").textContent = j.receipts_sent;
    $("#s-bar").style.transform = `scaleX(${clamp01(j.inr_spent_today / GLOBAL_CAP_INR).toFixed(3)})`;
    pill.hidden = !j.paused;
    cap.textContent = capped ? "Today's demo budget is used up · Zcash testnet"
      : `live · checked ${new Date().toLocaleTimeString("en-IN", { hour: "2-digit", minute: "2-digit", second: "2-digit" })} · Zcash testnet`;
  };
  const poll = () => fetch(`${base}/stats.json`, { cache: "no-store" })
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
    .then((j) => (valid(j) ? render(j) : offline()))
    .catch(offline);
  let timer = 0;
  const run = () => { clearInterval(timer); if (!document.hidden) { poll(); timer = setInterval(poll, STATS_POLL_MS); } };
  document.addEventListener("visibilitychange", run);
  run();
}

/* ---------- boot ---------- */
startChat();
startViewAs();
startReveals();
startTagline();
startCopy();
startStats();
