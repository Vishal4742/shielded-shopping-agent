// Landing page behaviour. No framework: glyph shader, chat loop, view-as toggle, scroll reveals, live stats.
"use strict";

const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s) => document.querySelector(s);
const clamp01 = (v) => Math.min(1, Math.max(0, v));

/* ---------- glyph shader ----------
   Downsample the source into one pixel per cell, then draw a Geist Mono glyph per cell:
   strong edges get a stroke along the edge, everything else a density ramp, tinted with the cell's colour. */
const RAMP = " ·:;+=xX%#";
const EDGE = "│╱─╲"; // box-drawing strokes by gradient angle 0, 45, 90, 135 degrees (geist-mono-blocks covers U+2500-259F)
const MONO_ADVANCE = 0.6; // Geist Mono advance width per em
const EDGE_MIN = 0.42, DARK = 0.06, INK = 1.3;
const FADE = 0.3; // cells dimmer than this fade out, so the floor glow has no hard edge

function makeGlyphRenderer(canvas) {
  const samp = document.createElement("canvas");
  const sctx = samp.getContext("2d", { willReadFrequently: true });
  const ctx = canvas.getContext("2d");
  let W = 0, H = 0, ch = 10, cw = 6, cols = 0, rows = 0;

  function resize() {
    const dpr = Math.min(2, devicePixelRatio || 1);
    W = canvas.clientWidth; H = canvas.clientHeight;
    canvas.width = Math.round(W * dpr); canvas.height = Math.round(H * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ch = W < 420 ? 8 : 10; cw = ch * MONO_ADVANCE;
    cols = Math.floor(W / cw); rows = Math.floor(H / ch);
    samp.width = cols; samp.height = rows;
  }

  // draw(src, sx, sy, sw, sh, rowLimit): rowLimit < rows draws only the top rows (the develop effect)
  function draw(src, sx, sy, sw, sh, rowLimit = Infinity) {
    if (!cols || !rows) return 0;
    sctx.imageSmoothingQuality = "high";
    sctx.drawImage(src, sx, sy, sw, sh, 0, 0, cols, rows);
    const d = sctx.getImageData(0, 0, cols, rows).data;
    const L = new Float32Array(cols * rows);
    for (let q = 0; q < L.length; q++) L[q] = (0.2126 * d[q * 4] + 0.7152 * d[q * 4 + 1] + 0.0722 * d[q * 4 + 2]) / 255;
    const at = (x, y) => L[Math.min(rows - 1, Math.max(0, y)) * cols + Math.min(cols - 1, Math.max(0, x))];
    const tint = (v) => Math.min(255, Math.round(v * INK));

    ctx.clearRect(0, 0, W, H);
    ctx.font = `500 ${ch}px "Geist Mono", monospace`;
    ctx.textBaseline = "top";
    let drawn = 0;
    const last = Math.min(rows, rowLimit);
    for (let j = 0; j < last; j++) for (let i = 0; i < cols; i++) {
      const q = j * cols + i, l = L[q];
      if (l < DARK) continue;
      const gx = at(i + 1, j - 1) + 2 * at(i + 1, j) + at(i + 1, j + 1) - at(i - 1, j - 1) - 2 * at(i - 1, j) - at(i - 1, j + 1);
      const gy = at(i - 1, j + 1) + 2 * at(i, j + 1) + at(i + 1, j + 1) - at(i - 1, j - 1) - 2 * at(i, j - 1) - at(i + 1, j - 1);
      const angle = (Math.atan2(gy, gx) + Math.PI) % Math.PI;
      const glyph = Math.hypot(gx, gy) > EDGE_MIN
        ? EDGE[Math.round(angle / (Math.PI / 4)) % 4]
        : RAMP[Math.min(RAMP.length - 1, Math.floor(l * RAMP.length))];
      const alpha = Math.min(1, ((l - DARK) / (FADE - DARK)) ** 2);
      ctx.fillStyle = `rgb(${tint(d[q * 4])} ${tint(d[q * 4 + 1])} ${tint(d[q * 4 + 2])} / ${alpha.toFixed(2)})`;
      ctx.fillText(glyph, i * cw, j * ch);
      drawn++;
    }
    return drawn;
  }

  resize();
  return { draw, resize, get rows() { return rows; }, get cols() { return cols; } };
}

function onVisible(el, cb, opts = {}) {
  const io = new IntersectionObserver((entries) => entries.forEach((e) => cb(e.isIntersecting, e)), opts);
  io.observe(el);
  return io;
}

function loadImage(src) {
  const img = new Image();
  img.src = src;
  return img.decode().then(() => img);
}

/* ---------- scan: glyphs resolve into the clear render as the section scrolls past ---------- */
function startScan(fontsReady) {
  const canvas = $("#scan-canvas"), img = $("#scan-solid"), cap = $("#scan-cap");
  if (!canvas || !img) return;
  const CLEAR = "With your viewing key: Jio ₹10 recharge, paid";
  if (reduced) { img.style.opacity = "1"; canvas.hidden = true; cap.textContent = CLEAR; return; }
  Promise.all([loadImage("solid.jpg"), fontsReady]).then(([solid]) => {
    const r = makeGlyphRenderer(canvas);
    const paint = () => r.draw(solid, 0, 0, solid.naturalWidth, solid.naturalHeight);
    paint();
    let pending = false;
    const update = () => {
      pending = false;
      const rect = canvas.getBoundingClientRect(), vh = innerHeight;
      // 0 while the figure's middle is below 60% of the screen, 1 when its middle reaches 30%
      const mid = rect.top + rect.height / 2;
      const p = clamp01((vh * 0.6 - mid) / (vh * 0.3));
      img.style.opacity = p.toFixed(3);
      canvas.style.opacity = (1 - p * 0.9).toFixed(3);
      cap.textContent = p >= 1 ? CLEAR : p <= 0 ? "On the blockchain: unreadable" : `Applying your viewing key · ${Math.round(p * 100)}%`;
    };
    const queue = () => { if (!pending) { pending = true; requestAnimationFrame(update); } };
    addEventListener("scroll", queue, { passive: true });
    addEventListener("resize", () => { r.resize(); paint(); queue(); });
    update();
  }).catch(() => { cap.textContent = "The artwork failed to load."; });
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
  chain: "On chain this is a shielded transaction: no number, no amount, no memo, and no link back to you.",
  bitrefill: "Bitrefill gets the number and amount it must deliver. It never sees your Zcash address or your Telegram account.",
  you: "With your viewing key every field opens, and the memo ties the purchase to your budget.",
};
const BLOCKS = "░▒▓█▚▞";
const SCRAMBLE_MS = 420;
const noise = (n) => Array.from({ length: n }, () => BLOCKS[(Math.random() * BLOCKS.length) | 0]).join("");

function startViewAs() {
  const tabs = [...document.querySelectorAll(".seg [data-view]")];
  const fields = [...document.querySelectorAll("#receipt dd")];
  const note = $("#viewas-note");
  if (!tabs.length) return;

  const setField = (dd, view) => {
    const target = dd.dataset[view];
    const hidden = target === "hidden";
    const finalText = hidden ? noise(12) : target;
    dd.classList.toggle("hidden", hidden);
    dd.setAttribute("aria-label", hidden ? "Encrypted" : target);
    if (reduced) { dd.textContent = finalText; return; }
    const t0 = performance.now();
    const step = (now) => { // resolve left to right through block glyphs
      const p = clamp01((now - t0) / SCRAMBLE_MS), keep = Math.floor(finalText.length * p);
      dd.textContent = finalText.slice(0, keep) + noise(Math.max(0, finalText.length - keep));
      if (p < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
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

/* ---------- report: Telegram Login Widget → GET {STATS_URL}/me with the signed fields ---------- */
const TG_FIELDS = ["id", "first_name", "last_name", "username", "photo_url", "auth_date", "hash"];
const STATE_LABEL = {
  DELIVERED: ["Delivered", "ok"], RECEIPT_SENDING: ["Delivered", "ok"], RECEIPTED: ["Delivered", "ok"],
  CONFIRMED: ["In progress", "wait"], PURCHASING: ["In progress", "wait"],
  UNCERTAIN: ["Checking with Bitrefill", "wait"], FAILED: ["Failed, refunded", "bad"],
};
const el = (tag, cls, text) => { const n = document.createElement(tag); if (cls) n.className = cls; if (text != null) n.textContent = text; return n; };
const istTime = (ts) => new Date(ts * 1000).toLocaleString("en-IN", { timeZone: "Asia/Kolkata", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
const opName = (op) => ({ jio: "Jio", airtel: "Airtel", vi: "Vi" })[op] || op;

function startReport() {
  const host = $("#tg-login"), out = $("#report-out");
  if (!host) return;
  const base = (window.SSA_CONFIG && window.SSA_CONFIG.STATS_URL || "").replace(/\/$/, "");
  const say = (...nodes) => out.replaceChildren(...nodes);
  const showWidget = () => {
    if (!base) { say(el("p", "note", "Reports need the live link to the bot, and it's offline right now. Send /report to the bot in Telegram instead.")); return; }
    const s = document.createElement("script");
    s.async = true;
    s.src = "https://telegram.org/js/telegram-widget.js?22";
    s.dataset.telegramLogin = "zcashdemo_bot";
    s.dataset.size = "large";
    s.dataset.radius = "8";
    s.dataset.authUrl = location.origin + location.pathname;
    s.onerror = () => say(el("p", "note", "The Telegram login button didn't load. Check your connection and reload."));
    host.replaceChildren(s);
  };
  const render = (me, who) => {
    if (!me.registered) {
      say(el("p", "hello", `Hi ${who}.`), el("p", "note", "This Telegram account isn't on the invite list yet. Get an invite code at our table, then send /start CODE to @zcashdemo_bot."));
      return;
    }
    const nodes = [el("p", "hello", `Hi ${who}. Here are your latest recharges.`)];
    if (!me.orders.length) nodes.push(el("p", "note", "No recharges yet. Send /auto jio <number> 10 weekly in Telegram to set up your first one."));
    else {
      const list = el("ul", "report-list");
      me.orders.forEach((o) => {
        const [label, tone] = STATE_LABEL[o.status] || [o.status, "wait"];
        const li = el("li");
        li.append(el("span", "what", `${opName(o.operator)} ₹${o.inr} → ${o.phone}`),
          el("span", `state ${tone}`, label),
          el("span", "meta", `${istTime(o.at)}${o.receipt ? " · shielded receipt sent" : ""}`));
        list.append(li);
      });
      nodes.push(list);
    }
    if (me.rules.length) {
      nodes.push(el("h3", null, "Auto-pay rules"));
      const list = el("ul", "report-list");
      me.rules.forEach((r) => {
        const li = el("li");
        li.append(el("span", "what", `#${r.id} · ${opName(r.operator)} ₹${r.inr} → ${r.phone}`), el("span", "state ok", "On"), el("span", "meta", r.every));
        list.append(li);
      });
      nodes.push(list);
    }
    const out_ = el("button", "btn-link", "Log out");
    out_.type = "button";
    out_.addEventListener("click", () => { say(); showWidget(); });
    nodes.push(out_);
    say(...nodes);
  };

  const params = new URLSearchParams(location.search);
  if (!(params.has("hash") && params.has("id"))) { showWidget(); return; }
  // Came back from Telegram: keep only the signed fields (in memory, never stored), then clean the address bar.
  const q = new URLSearchParams(TG_FIELDS.filter((k) => params.has(k)).map((k) => [k, params.get(k)]));
  const who = params.get("first_name") || "there";
  history.replaceState(null, "", location.pathname + "#report");
  $("#report").scrollIntoView();
  if (!base) { showWidget(); return; }
  say(el("p", "note", "Loading your recharges…"));
  fetch(`${base}/me`, { cache: "no-store", headers: { Authorization: `tg ${q}` } })
    .then((r) => (r.status === 401 ? Promise.reject(new Error("login")) : r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
    .then((me) => render(me, who))
    .catch((e) => {
      say(el("p", "note", e.message === "login" ? "That login expired (they last an hour). Log in again." : "Couldn't reach the bot right now. Try again in a minute, or send /report in Telegram."));
      showWidget();
    });
}

/* ---------- boot ---------- */
// canvas text doesn't trigger unicode-range subsets, so load the block/box subset explicitly
const fontsReady = document.fonts.load('500 10px "Geist Mono"', `${RAMP}${EDGE}`).catch(() => null);
startScan(fontsReady);
startChat();
startViewAs();
startReveals();
startTagline();
startCopy();
startStats();
startReport();
