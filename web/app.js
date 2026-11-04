// Landing page behaviour. No framework: glyph shader, chat loop, view-as toggle, scroll reveals, live stats.
"use strict";

const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s) => document.querySelector(s);
const clamp01 = (v) => Math.min(1, Math.max(0, v));

/* ---------- glyph shader ----------
   Downsample the source into one pixel per cell, then draw a Geist Mono glyph per cell:
   strong edges get a stroke along the edge, everything else a density ramp, tinted with the cell's colour. */
const RAMP = " .:-=+*#%@";
const EDGE = "|/-\\"; // gradient angle 0, 45, 90, 135 degrees
const MONO_ADVANCE = 0.6; // Geist Mono advance width per em
const EDGE_MIN = 0.42, DARK = 0.085, INK = 1.3; // DARK above the floor's glow, so the floor plane doesn't draw a dotted box

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
      ctx.fillStyle = `rgb(${tint(d[q * 4])},${tint(d[q * 4 + 1])},${tint(d[q * 4 + 2])})`;
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

/* ---------- hero: 96 frame turntable from a 12 x 8 sprite sheet ---------- */
const SPRITE = { cols: 12, rows: 8, w: 200, h: 250, frames: 96, fps: 24 };
const DEVELOP_MS = 900;

function startHero(fontsReady) {
  const canvas = $("#hero-canvas"), cap = $("#hero-cap");
  if (!canvas) return;
  Promise.all([loadImage("sprite.jpg"), fontsReady]).then(([sprite]) => {
    const r = makeGlyphRenderer(canvas);
    const frameAt = (k) => [(k % SPRITE.cols) * SPRITE.w, Math.floor(k / SPRITE.cols) * SPRITE.h];
    const paint = (k, rowLimit) => { const [sx, sy] = frameAt(k); return r.draw(sprite, sx, sy, SPRITE.w, SPRITE.h, rowLimit); };
    cap.textContent = `glyph shader · ${r.cols} × ${r.rows} cells · ${SPRITE.frames} frames`;

    if (reduced) {
      paint(0);
      addEventListener("resize", () => { r.resize(); paint(0); });
      return;
    }
    let running = false, raf = 0, t0 = performance.now(), lastFrame = -1;
    const tick = (now) => {
      const elapsed = now - t0;
      const k = Math.floor((elapsed / 1000) * SPRITE.fps) % SPRITE.frames;
      const developing = elapsed < DEVELOP_MS;
      if (k !== lastFrame || developing) {
        const p = clamp01(elapsed / DEVELOP_MS), eased = 1 - Math.pow(1 - p, 3);
        paint(k, developing ? Math.ceil(r.rows * eased) : Infinity);
        lastFrame = k;
      }
      if (running) raf = requestAnimationFrame(tick);
    };
    const play = (on) => {
      if (on === running) return;
      running = on;
      if (on) raf = requestAnimationFrame(tick); else cancelAnimationFrame(raf);
    };
    let inView = true;
    onVisible(canvas, (v) => { inView = v; play(v && !document.hidden); });
    document.addEventListener("visibilitychange", () => play(inView && !document.hidden));
    addEventListener("resize", () => { r.resize(); lastFrame = -1; });
  }).catch(() => { cap.textContent = "The artwork failed to load. Reload the page to try again."; });
}

/* ---------- scan: glyphs resolve into the clear render as the section scrolls past ---------- */
function startScan(fontsReady) {
  const canvas = $("#scan-canvas"), img = $("#scan-solid"), cap = $("#scan-cap");
  if (!canvas || !img) return;
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
      cap.textContent = p >= 1 ? "viewing key applied" : `scan · ${Math.round(p * 100)}%`;
    };
    const queue = () => { if (!pending) { pending = true; requestAnimationFrame(update); } };
    addEventListener("scroll", queue, { passive: true });
    addEventListener("resize", () => { r.resize(); paint(); queue(); });
    update();
  }).catch(() => { cap.textContent = "The artwork failed to load."; });
}

/* ---------- chat: scripted loop while on screen ---------- */
const CHAT_SCRIPT = [ // [ms from start, action]
  [300, 0], [1300, 1], [2700, "press"], [3100, 2], [4700, 3], [5700, 4],
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
  chain: "The chain sees an encrypted transaction. No phone, no amount, no memo, nothing that points to you.",
  bitrefill: "Bitrefill sees the phone number and the amount. Not your wallet, your balance or your other purchases.",
  you: "You see everything. Your viewing key decrypts the memo and links it to your budget.",
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
const STATS_STALE_S = 60;
const GLOBAL_CAP_INR = 200; // BR-3
const STATS_KEYS = ["delivered_today", "inr_today", "inr_cap_today", "receipts_total", "paused", "as_of"];

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
    cap.textContent = "Live stats offline. The bot runs on a laptop at the venue.";
  };
  if (!base) { offline(); return; }

  // ponytail: also accepts zcash-3f's proposed shape (no as_of, no cap) until one contract wins; drop the branch then
  const normalize = (j) => (j && !("delivered_today" in j) && "orders_delivered" in j
    ? { delivered_today: j.orders_delivered, inr_today: j.inr_spent_today, inr_cap_today: GLOBAL_CAP_INR,
        receipts_total: j.receipts_sent, paused: j.paused, as_of: Math.floor(Date.now() / 1000) }
    : j);
  const valid = (j) => j && STATS_KEYS.every((k) => k in j)
    && ["delivered_today", "inr_today", "inr_cap_today", "receipts_total", "as_of"].every((k) => Number.isInteger(j[k]) && j[k] >= 0)
    && typeof j.paused === "boolean";
  const render = (j) => {
    const age = Math.max(0, Math.round(Date.now() / 1000 - j.as_of));
    if (age > STATS_STALE_S) { offline(); return; }
    const capped = j.inr_today >= j.inr_cap_today;
    panel.dataset.state = capped ? "capped" : "live";
    $("#s-delivered").textContent = j.delivered_today;
    $("#s-inr").textContent = `₹${j.inr_today}`;
    $("#s-receipts").textContent = j.receipts_total;
    $("#s-bar").style.transform = `scaleX(${clamp01(j.inr_today / j.inr_cap_today).toFixed(3)})`;
    pill.hidden = !j.paused;
    cap.textContent = capped ? "Today's demo budget is used up · Zcash testnet" : `updated ${age} s ago · Zcash testnet`;
  };
  const poll = () => fetch(`${base}/stats.json`, { cache: "no-store" })
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
    .then((raw) => { const j = normalize(raw); return valid(j) ? render(j) : offline(); })
    .catch(offline);
  let timer = 0;
  const run = () => { clearInterval(timer); if (!document.hidden) { poll(); timer = setInterval(poll, STATS_POLL_MS); } };
  document.addEventListener("visibilitychange", run);
  run();
}

/* ---------- boot ---------- */
const fontsReady = document.fonts.load('500 10px "Geist Mono"').catch(() => null);
startHero(fontsReady);
startScan(fontsReady);
startChat();
startViewAs();
startReveals();
startTagline();
startCopy();
startStats();
