// Snook Legend staff portal. Plain JavaScript, no dependencies, no inline
// code or styles (the page's security policy forbids them). Every value from
// the server goes into the page as text (textContent), never as HTML.
"use strict";

// ===================================================================== basics

const $ = (sel, root = document) => root.querySelector(sel);
const KEYS = { token: "snook-legend-admin", lang: "snook-legend-admin-lang", theme: "snook-legend-admin-theme" };
const LANGS = [["pt-BR", "Português (Brasil)"], ["en", "English"], ["es", "Español"]];
const DICT = typeof WORDS === "undefined" ? {} : WORDS;
const store = {
  get(k) { try { return localStorage.getItem(k); } catch (_) { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch (_) { /* private mode */ } },
};
let token = (() => { try { return sessionStorage.getItem(KEYS.token) || ""; } catch (_) { return ""; } })();
let me = null;
let lang = pickLang();
let theme = store.get(KEYS.theme) || (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark");

function pickLang() {
  const saved = store.get(KEYS.lang);
  if (saved && LANGS.some(([c]) => c === saved)) return saved;
  for (const l of navigator.languages || [navigator.language || "en"]) {
    const s = String(l).toLowerCase();
    if (s.startsWith("pt")) return "pt-BR";
    if (s.startsWith("es")) return "es";
    if (s.startsWith("en")) return "en";
  }
  return "en";
}

/** A text in the chosen language (English is the key). */
function t(s) {
  const d = DICT[lang];
  return (d && d[s]) || s;
}
/** A text with {placeholders}. */
function tf(s, vars) {
  return t(s).replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
}
/** "1 game" / "5 games": `one` and `many` carry {n}. */
function tn(n, one, many) {
  return tf(n === 1 ? one : many, { n: fmtNum(n) });
}

function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k.startsWith("on") && typeof v === "function") e.addEventListener(k.slice(2), v);
    else if (k === "value") e.value = v;
    else if (v === true) e.setAttribute(k, "");
    else e.setAttribute(k, v);
  }
  append(e, children);
  return e;
}
/** Replaces a node's children (arrays flattened, empty values skipped). */
function setKids(node, ...children) {
  node.replaceChildren();
  return append(node, children);
}
function append(e, children) {
  for (const c of children.flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return e;
}
let lastId = 0;
/** An id for a label to point at. */
const newId = (prefix) => prefix + "-" + ++lastId;
/** The server drops line breaks (gluing the words), so a text typed on
 * several lines goes as one, with spaces. */
const oneLine = (s) => String(s ?? "").replace(/\s*\n\s*/g, " ");
/** An input or text area the user types in (not a checkbox or a slider). */
function isTextField(x) {
  if (!x) return false;
  if (x.tagName === "TEXTAREA") return true;
  return x.tagName === "INPUT" && !["checkbox", "radio", "range", "button", "submit", "reset", "file", "color"].includes(x.type);
}
const SVGNS = "http://www.w3.org/2000/svg";
function svg(tag, attrs = {}, ...children) {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs)) if (v !== null && v !== undefined) e.setAttribute(k, v);
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return e;
}

// ===================================================================== icons

const circle = (cx, cy, r) => `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
const ICONS = {
  dashboard: "M3 3h7v9H3zM14 3h7v5h-7zM14 12h7v9h-7zM3 16h7v5H3z",
  users: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" + circle(9, 7, 4) + "M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75",
  user: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" + circle(12, 7, 4),
  flag: "M4 22V4M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1",
  activity: "M22 12h-4l-3 9L9 3l-3 9H2",
  chat: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
  megaphone: "M3 11v2a1 1 0 0 0 1 1h2l5 4V6L6 10H4a1 1 0 0 0-1 1zM15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13",
  coins: circle(8, 8, 6) + "M18.09 10.37A6 6 0 1 1 10.34 18M7 6h1v4M16.71 13.88l.7.71-2.82 2.82",
  chart: "M3 3v18h18M7 16v-5M12 16V8M17 16v-9",
  target: circle(12, 12, 10) + circle(12, 12, 6) + circle(12, 12, 2),
  audit: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M16 13H8M16 17H8M10 9H8",
  server: "M4 3h16a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM4 14h16a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1zM7 6.5h.01M7 17.5h.01",
  search: circle(11, 11, 8) + "M21 21l-4.35-4.35",
  sun: circle(12, 12, 4) + "M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41",
  moon: "M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z",
  logout: "M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4M16 17l5-5-5-5M21 12H9",
  menu: "M3 12h18M3 6h18M3 18h18",
  x: "M18 6L6 18M6 6l12 12",
  chevronRight: "M9 18l6-6-6-6",
  chevronLeft: "M15 18l-6-6 6-6",
  chevronDown: "M6 9l6 6 6-6",
  refresh: "M23 4v6h-6M1 20v-6h6M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15",
  eye: "M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" + circle(12, 12, 3),
  eyeOff: "M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19M1 1l22 22M14.12 14.12a3 3 0 1 1-4.24-4.24",
  play: "M5 3l14 9-14 9V3z",
  pause: "M6 4h4v16H6zM14 4h4v16h-4z",
  download: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3",
  ban: circle(12, 12, 10) + "M4.93 4.93l14.14 14.14",
  mute: "M1 1l22 22M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23M12 19v4M8 23h8",
  warn: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01",
  note: "M14.5 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9.5zM14 3v6h6M8 13h8M8 17h5",
  kick: "M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" + circle(8.5, 7, 4) + "M18 8l5 5M23 8l-5 5",
  shield: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
  key: "M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.78 7.78 5.5 5.5 0 0 1 7.78-7.78zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4",
  gift: "M20 12v10H4V12M2 7h20v5H2zM12 22V7M12 7H7.5a2.5 2.5 0 0 1 0-5C11 2 12 7 12 7zM12 7h4.5a2.5 2.5 0 0 0 0-5C13 2 12 7 12 7z",
  edit: "M12 20h9M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4z",
  trash: "M3 6h18M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2",
  copy: "M9 9h11a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-9a2 2 0 0 1-2-2zM5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1",
  check: "M20 6L9 17l-5-5",
  checkCircle: circle(12, 12, 10) + "M8 12l3 3 5-6",
  clock: circle(12, 12, 10) + "M12 6v6l4 2",
  crown: "M2 18h20M3 7l4 4 5-7 5 7 4-4-2 11H5z",
  trophy: "M8 21h8M12 17v4M7 4h10v5a5 5 0 0 1-10 0zM17 5h3v2a3 3 0 0 1-3 3M7 5H4v2a3 3 0 0 0 3 3",
  info: circle(12, 12, 10) + "M12 16v-4M12 8h.01",
  more: circle(5, 12, 1) + circle(12, 12, 1) + circle(19, 12, 1),
  pin: "M12 17v5M9 10.76V6h6v4.76a2 2 0 0 0 1.11 1.79l1.78.9A2 2 0 0 1 19 15.24V17H5v-1.76a2 2 0 0 1 1.11-1.79l1.78-.9A2 2 0 0 0 9 10.76zM8 2h8",
  keyboard: "M2 6h20v12H2zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10",
  send: "M22 2L11 13M22 2l-7 20-4-9-9-4z",
  lock: "M5 11h14v10H5zM8 11V7a4 4 0 0 1 8 0v4",
  star: "M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z",
  calendar: "M3 4h18v18H3zM16 2v4M8 2v4M3 10h18",
  undo: "M3 7v6h6M21 17a9 9 0 0 0-15-6.7L3 13",
  globe: circle(12, 12, 10) + "M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z",
  table: "M3 3h18v18H3zM3 9h18M3 15h18M9 3v18",
  queue: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  memory: "M6 19v2M10 19v2M14 19v2M18 19v2M8 11h.01M16 11h.01M2 6h20v13H2z",
  plug: "M12 22v-5M9 8V2M15 8V2M18 8v5a6 6 0 0 1-12 0V8z",
};
function icon(name, cls = "") {
  const s = svg("svg", { viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": "2",
    "stroke-linecap": "round", "stroke-linejoin": "round", class: ("icon " + cls).trim(), "aria-hidden": "true" });
  s.append(svg("path", { d: ICONS[name] || ICONS.info }));
  return s;
}
function brandMark() {
  return svg("svg", { viewBox: "0 0 32 32", class: "brand-mark", "aria-hidden": "true" },
    svg("circle", { cx: 16, cy: 16, r: 15, fill: "#141414" }),
    svg("circle", { cx: 16, cy: 16, r: 7.5, fill: "#f8f3e6" }),
    svg("text", { x: 16, y: 20, "font-size": 10.5, "font-weight": 800, "text-anchor": "middle", fill: "#141414", "font-family": "Arial, sans-serif" }, "8"));
}

// ================================================================ formatting

const fmtNum = (n) => new Intl.NumberFormat(lang).format(Number(n) || 0);
const fmtPct = (x, digits = 1) => new Intl.NumberFormat(lang, { style: "percent", maximumFractionDigits: digits }).format((Number(x) || 0) / 100);
const fmtDate = (s) => (s ? new Intl.DateTimeFormat(lang, { dateStyle: "medium", timeStyle: "short" }).format(new Date(s * 1000)) : "—");
const fmtDateOnly = (s) => (s ? new Intl.DateTimeFormat(lang, { dateStyle: "medium" }).format(new Date(s * 1000)) : "—");
const fmtClock = (s) => new Intl.DateTimeFormat(lang, { hour: "2-digit", minute: "2-digit" }).format(new Date(s * 1000));
const fmtClockSecs = (d) => new Intl.DateTimeFormat(lang, { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(d);
const fmtDay = (day) => new Intl.DateTimeFormat(lang, { day: "2-digit", month: "2-digit", timeZone: "UTC" }).format(new Date(day * 86400000));
const fmtDayLong = (day) => new Intl.DateTimeFormat(lang, { dateStyle: "medium", timeZone: "UTC" }).format(new Date(day * 86400000));
const nowSecs = () => Date.now() / 1000;
const FOREVER = 9e15;

function ago(s) {
  if (!s) return "—";
  const d = s - nowSecs();
  const rtf = new Intl.RelativeTimeFormat(lang, { numeric: "auto" });
  const a = Math.abs(d);
  if (a < 45) return rtf.format(0, "second");
  if (a < 3600) return rtf.format(Math.round(d / 60), "minute");
  if (a < 86400) return rtf.format(Math.round(d / 3600), "hour");
  if (a < 86400 * 45) return rtf.format(Math.round(d / 86400), "day");
  if (a < 86400 * 400) return rtf.format(Math.round(d / (86400 * 30)), "month");
  return rtf.format(Math.round(d / (86400 * 365)), "year");
}
function timeEl(s) {
  return el("time", { datetime: s ? new Date(s * 1000).toISOString() : null, title: fmtDate(s) }, ago(s));
}
function fmtDuration(secs) {
  secs = Math.max(0, Math.round(secs));
  const d = Math.floor(secs / 86400), h = Math.floor((secs % 86400) / 3600), m = Math.floor((secs % 3600) / 60);
  if (d) return tf("{d} d {h} h", { d, h });
  if (h) return tf("{h} h {m} min", { h, m });
  if (m) return tf("{m} min", { m });
  return tf("{s} s", { s: secs });
}
function fmtMinutes(min) {
  if (min % 10080 === 0) return tn(min / 10080, "{n} week", "{n} weeks");
  if (min % 1440 === 0) return tn(min / 1440, "{n} day", "{n} days");
  if (min % 60 === 0) return tn(min / 60, "{n} hour", "{n} hours");
  return tn(min, "{n} minute", "{n} minutes");
}
function fmtBytes(b) {
  if (!b && b !== 0) return "—";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
  return new Intl.NumberFormat(lang, { maximumFractionDigits: i ? 1 : 0 }).format(b) + " " + u[i];
}
function money(cents, currency) {
  if (currency === "msstore") return "Microsoft Store";
  try { return new Intl.NumberFormat(lang, { style: "currency", currency: currency.toUpperCase() }).format(cents / 100); }
  catch (_) { return (cents / 100).toFixed(2) + " " + currency.toUpperCase(); }
}
/** A country's flag, from the game's own atlas (see /admin/flags.css): the
 *  flag emoji aren't drawn everywhere (Windows shows the two letters). */
function flagOf(code, cls = "") {
  if (!/^[A-Za-z]{2}$/.test(code || "")) return null;
  return el("span", { class: ("fl fl-" + code.toLowerCase() + " " + cls).trim(), "aria-hidden": "true" });
}
function countryName(code) {
  if (!code) return t("No country");
  try { return new Intl.DisplayNames([lang], { type: "region" }).of(code.toUpperCase()) || code; }
  catch (_) { return code; }
}
function country(code) {
  return el("span", { class: "nowrap", title: code || "" },
    flagOf(code), code ? " " : null, countryName(code));
}
function hue(name) {
  let h = 0;
  for (const c of String(name)) h = (h * 31 + c.codePointAt(0)) % 360;
  return h;
}
function avatar(name, size = "") {
  const a = el("span", { class: ("avatar " + size).trim(), "aria-hidden": "true" }, String(name || "?").slice(0, 1));
  a.style.background = `hsl(${hue(name)} 38% 38%)`;
  return a;
}
function personLink(name, opts = {}) {
  if (!name) return el("span", { class: "faint" }, "—");
  return el("span", { class: "person" }, opts.avatar === false ? null : avatar(name, "sm"),
    el("a", { class: "name", href: "#/players/" + encodeURIComponent(name), onclick: (e) => e.stopPropagation() }, name));
}
function roleLabel(role) {
  return { admin: t("Administrator"), moderator: t("Moderator"), player: t("Player") }[role] || role;
}
function tierLabel(tier) { return t(tier); }
function badge(text, kind = "", ic) {
  return el("span", { class: ("badge " + kind).trim() }, ic ? icon(ic) : null, text);
}
function userBadges(u, { online = true } = {}) {
  const now = nowSecs();
  return el("span", { class: "badges" },
    online && u.online ? badge(t("Online"), "success dot") : null,
    u.role && u.role !== "player" ? badge(roleLabel(u.role), u.role === "admin" ? "danger" : "info", "shield") : null,
    u.banned_until && u.banned_until > now ? badge(t("Banned"), "danger", "ban") : null,
    u.muted_until && u.muted_until > now ? badge(t("Muted"), "warning", "mute") : null,
    u.deleted_at ? badge(t("Deleted"), "") : null);
}
const isAdmin = () => !!me && me.role === "admin";

// ===================================================================== rules

/** The two rules every game is played under, Brazilian first (as the
 * server lists them). Each has its own rating, tiers and ranking. */
const RULES = ["brazilian", "official"];
const rulesName = (r) => (r === "official" ? t("Official rules") : t("Brazilian rules"));
const rulesShort = (r) => (r === "official" ? t("Official") : t("Brazilian"));
/** The rules' mark, as in the game: Brazil's flag, or a globe. */
function rulesMark(r) {
  return r === "official" ? icon("globe", "rules-mark") : flagOf("BR", "rules-mark");
}
function rulesBadge(r, long = false) {
  const k = r === "official" ? "official" : "brazilian";
  return el("span", { class: "badge rules " + k, title: rulesName(k) }, rulesMark(k), long ? rulesName(k) : rulesShort(k));
}
/** A player's record under the rules: an account's `records`, or a player
 * online's `standings`. */
function recordOf(p, r) {
  const x = (p.records && p.records[r]) || (p.standings && p.standings[r]) || {};
  return { rating: x.rating ?? 1200, peak_rating: x.peak_rating ?? x.rating ?? 1200, tier: x.tier || "", games: x.games || 0, wins: x.wins || 0 };
}
const winRate = (x, digits = 0) => (x.games ? fmtPct(x.wins * 100 / x.games, digits) : "—");
/** Both ratings side by side, each with its mark (lists, the search). */
function ratingPair(p) {
  return el("span", { class: "rating-pair" }, RULES.map((r) => {
    const x = recordOf(p, r);
    return el("span", { class: "rp" + (x.games ? "" : " unplayed"), title: rulesName(r) + " · " + tn(x.games, "{n} rated game", "{n} rated games") }, rulesMark(r), fmtNum(x.rating));
  }));
}
/** A number under each of the rules, in words ("3 Brazilian · 1 official"). */
const perRulesText = (n) => tf("{brazilian} Brazilian · {official} official", { brazilian: fmtNum((n && n.brazilian) || 0), official: fmtNum((n && n.official) || 0) });

// ======================================================================= api

class ApiError extends Error {
  constructor(message, status) { super(message); this.status = status; }
}
async function api(path, options = {}) {
  let res;
  try {
    res = await fetch("/admin/api" + path, {
      method: options.method || "GET",
      headers: { "Content-Type": "application/json", ...(token ? { Authorization: "Bearer " + token } : {}) },
      body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
      cache: "no-store",
    });
  } catch (_) {
    throw new ApiError(t("Can't reach the server. Check the connection and try again."), 0);
  }
  let data = {};
  try { data = await res.json(); } catch (_) { /* no body, or not JSON */ }
  if (!data || typeof data !== "object") data = {};
  // The server's own words when it sent them. A body that isn't JSON (a
  // request the server couldn't read, a proxy's page) gets the general one.
  const said = typeof data.error === "string" && data.error ? data.error : null;
  if (res.status === 401 && path !== "/login") {
    sessionExpired();
    throw new ApiError(said ? t(said) : t("Your session expired."), 401);
  }
  if (!res.ok) throw new ApiError(said ? t(said) : tf("Something went wrong ({status}).", { status: res.status }), res.status);
  return data;
}
/** Runs `fn` with `button` off, so a second click doesn't send it twice. */
async function busy(button, fn) {
  if (button.disabled) return undefined;
  const hadFocus = document.activeElement === button;
  button.disabled = true;
  try {
    return await fn();
  } finally {
    button.disabled = false;
    if (hadFocus && document.activeElement === document.body && document.contains(button)) button.focus();
  }
}
/** Runs an action: a toast for its outcome, true if it worked. */
async function attempt(fn, done) {
  try {
    await fn();
    if (done) toast(done, "success");
    return true;
  } catch (e) {
    toast(e.message, "error");
    return false;
  }
}

// ==================================================================== toasts

function toast(text, kind = "success") {
  const box = $("#toasts");
  const ic = kind === "error" ? "warn" : kind === "info" ? "info" : "checkCircle";
  const close = el("button", { class: "btn ghost sm icon-only", "aria-label": t("Close") }, icon("x"));
  const node = el("div", { class: "toast " + kind, role: kind === "error" ? "alert" : "status" }, icon(ic), el("div", { class: "toast-text" }, text), close);
  const remove = () => { node.classList.add("out"); setTimeout(() => node.remove(), 200); };
  close.addEventListener("click", remove);
  box.append(node);
  while (box.children.length > 4) box.firstChild.remove();
  setTimeout(remove, kind === "error" ? 7000 : 4200);
}

// =================================================================== dialogs

const openDialogs = [];
function dialog({ title, text, iconName, tone = "", body = [], actions = [], wide = false, onClose }) {
  const opener = document.activeElement;
  const closeBtn = el("button", { class: "btn ghost sm icon-only", "aria-label": t("Close") }, icon("x"));
  const foot = el("div", { class: "dialog-foot" });
  const box = el("div", { class: "dialog" + (wide ? " wide" : ""), role: "dialog", "aria-modal": "true", "aria-label": title },
    el("div", { class: "dialog-head" },
      iconName ? el("div", { class: ("dialog-icon " + tone).trim() }, icon(iconName)) : null,
      el("div", { class: "grow" }, el("h2", {}, title), text ? el("p", {}, text) : null),
      closeBtn),
    el("div", { class: "dialog-body" }, body),
    actions.length ? foot : null);
  const scrim = el("div", { class: "dialog-scrim" }, box);
  const handle = { root: box, scrim, close, closed: false };
  function close(result) {
    if (handle.closed) return;
    handle.closed = true;
    scrim.remove();
    openDialogs.splice(openDialogs.indexOf(handle), 1);
    if (opener && opener.focus && document.contains(opener)) opener.focus();
    if (onClose) onClose(result);
  }
  for (const a of actions) {
    const b = el("button", { class: "btn " + (a.kind || ""), type: "button" }, a.icon ? icon(a.icon) : null, a.label);
    b.addEventListener("click", () => a.onClick(handle, b));
    foot.append(b);
    if (a.primary) handle.primary = b;
    if (a.focus) handle.focusFirst = b;
  }
  closeBtn.addEventListener("click", () => close(false));
  scrim.addEventListener("mousedown", (e) => { if (e.target === scrim) close(false); });
  $("#dialogs").append(scrim);
  openDialogs.push(handle);
  const first = handle.focusFirst || box.querySelector("input:not([type=checkbox]), textarea, select") || handle.primary || closeBtn;
  setTimeout(() => first.focus(), 20);
  return handle;
}
/** The window on top that keeps the keyboard: the replay, or the last dialog. */
function topModal() {
  if (viewer) return viewer.box;
  const top = openDialogs[openDialogs.length - 1];
  return top ? top.root : null;
}
function trapFocus(e) {
  const root = topModal();
  if (!root || e.key !== "Tab") return;
  const items = [...root.querySelectorAll("button, input, select, textarea, a[href], [tabindex]:not([tabindex='-1'])")]
    .filter((x) => !x.disabled && x.offsetParent !== null && x.getAttribute("tabindex") !== "-1");
  if (!items.length) { e.preventDefault(); return; }
  const first = items[0], last = items[items.length - 1];
  const active = document.activeElement;
  // Focus that fell out of the window (a part of it was rebuilt, or it was
  // never in) comes back in, instead of going to the page behind.
  if (active === root || !root.contains(active)) { e.preventDefault(); (e.shiftKey ? last : first).focus(); }
  else if (e.shiftKey && active === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && active === last) { e.preventDefault(); first.focus(); }
}

function confirmDialog({ title, text, confirm, tone = "danger", iconName = "warn", details }) {
  return new Promise((resolve) => {
    dialog({
      title, text, iconName, tone,
      body: details ? [details] : [],
      onClose: (r) => resolve(!!r),
      actions: [
        // For something that can't be taken back, Enter means "Cancel".
        { label: t("Cancel"), focus: tone === "danger", onClick: (d) => d.close(false) },
        { label: confirm || t("Confirm"), kind: tone === "danger" ? "danger" : "primary", primary: true, onClick: (d) => d.close(true) },
      ],
    });
  });
}

/** A form in a dialog. Each field: { name, label, type, ... }; `onSubmit`
 * gets the values and throws to show an error (the dialog stays open). */
function formDialog({ title, text, iconName, tone, fields, submit, submitKind = "primary", onSubmit, wide, extra }) {
  const inputs = {};
  const errorBox = el("div", { class: "alert danger hidden", role: "alert" });
  const body = [];
  for (const f of fields) {
    if (f.type === "node") { body.push(f.node); continue; }
    const id = "f-" + f.name + "-" + Math.random().toString(36).slice(2, 7);
    let input;
    const counter = f.max ? el("span", { class: "counter" }) : null;
    if (f.type === "textarea") {
      input = el("textarea", { class: "textarea", id, rows: f.rows || 3, placeholder: f.placeholder || "", maxlength: f.max ? f.max * 2 : null });
    } else if (f.type === "select") {
      input = el("select", { class: "select", id }, f.options.map(([v, l]) => el("option", { value: v }, l)));
    } else if (f.type === "choices") {
      // A row of chips; the value is the chosen one.
      input = el("div", { class: "chips", role: "radiogroup", id, "aria-label": f.label || null });
      input.value = f.value ?? f.options[0][0];
      for (const [v, l] of f.options) {
        const chip = el("button", { type: "button", class: "chip" + (String(v) === String(input.value) ? " on" : ""), role: "radio", "aria-checked": String(String(v) === String(input.value)) }, l);
        chip.addEventListener("click", () => {
          input.value = v;
          input.querySelectorAll(".chip").forEach((c) => { c.classList.toggle("on", c === chip); c.setAttribute("aria-checked", String(c === chip)); });
          input.dispatchEvent(new Event("change"));
        });
        input.append(chip);
      }
    } else if (f.type === "checkbox") {
      input = el("input", { type: "checkbox", id });
      input.checked = !!f.value;
    } else {
      input = el("input", { class: "input", id, type: f.type || "text", placeholder: f.placeholder || "", autocomplete: f.autocomplete || "off",
        min: f.min, max: f.type === "number" ? f.maxValue : null, step: f.step });
    }
    if (f.value !== undefined && f.type !== "choices" && f.type !== "checkbox") input.value = f.value;
    inputs[f.name] = input;
    const updateCount = () => {
      if (!counter) return;
      const n = [...input.value].length;
      counter.textContent = fmtNum(n) + " / " + fmtNum(f.max);
      counter.classList.toggle("over", n > f.max);
    };
    if (counter) { input.addEventListener("input", updateCount); updateCount(); }
    if (f.onInput) input.addEventListener(f.type === "choices" ? "change" : "input", () => f.onInput(input.value, inputs));
    if (f.type === "checkbox") {
      body.push(el("label", { class: "check", for: id }, input, f.label));
    } else {
      body.push(el("div", { class: "field" },
        f.label ? el("label", { class: "label", for: f.type === "choices" ? null : id }, el("span", {}, f.label, f.optional ? el("span", { class: "faint" }, " · " + t("optional")) : null), counter) : null,
        input,
        f.hint ? el("div", { class: "hint" }, f.hint) : null));
    }
  }
  if (extra) body.push(extra);
  body.push(errorBox);
  const values = () => Object.fromEntries(Object.entries(inputs).map(([k, i]) => [k, i.type === "checkbox" ? i.checked : i.value]));
  const run = async (d, button) => {
    errorBox.classList.add("hidden");
    for (const f of fields) {
      if (f.type === "node") continue;
      const v = inputs[f.name].value;
      let problem = null;
      if (f.required && !String(v).trim()) problem = tf("Fill in “{field}”.", { field: f.label || f.name });
      else if (f.max && [...String(v)].length > f.max) problem = tf("“{field}” is too long.", { field: f.label });
      else if (f.validate) problem = f.validate(v, values());
      inputs[f.name].classList && inputs[f.name].classList.toggle("invalid", !!problem);
      if (problem) {
        setKids(errorBox, icon("warn"), el("div", {}, problem));
        errorBox.classList.remove("hidden");
        inputs[f.name].focus && inputs[f.name].focus();
        return;
      }
    }
    const label = button.textContent;
    button.disabled = true;
    setKids(button, el("span", { class: "spinner" }), label);
    try {
      await onSubmit(values(), d);
      d.close(true);
    } catch (e) {
      setKids(errorBox, icon("warn"), el("div", {}, e.message));
      errorBox.classList.remove("hidden");
    } finally {
      button.disabled = false;
      button.textContent = label;
    }
  };
  const d = dialog({
    title, text, iconName, tone, body, wide,
    actions: [
      { label: t("Cancel"), onClick: (h) => h.close(false) },
      { label: submit || t("Save"), kind: submitKind, primary: true, onClick: (h, b) => run(h, b) },
    ],
  });
  d.root.addEventListener("keydown", (e) => {
    const tag = e.target.tagName;
    if (e.key === "Enter" && (tag === "INPUT" && e.target.type !== "checkbox" || (tag === "TEXTAREA" && (e.ctrlKey || e.metaKey)))) {
      e.preventDefault();
      d.primary.click();
    }
  });
  return d;
}

// ===================================================================== menus

let openMenuNode = null;
function closeMenu() {
  if (openMenuNode) { openMenuNode.remove(); openMenuNode = null; }
}
/** A menu under `anchor`: items { label, icon, danger, onClick } or "-" or { group }. */
function openMenu(anchor, items) {
  closeMenu();
  const m = el("div", { class: "menu", role: "menu" });
  for (const it of items) {
    if (!it) continue;
    if (it === "-") { m.append(el("hr")); continue; }
    if (it.group) { m.append(el("div", { class: "menu-label" }, it.group)); continue; }
    m.append(el("button", { role: "menuitem", class: it.danger ? "danger" : "", onclick: () => { closeMenu(); it.onClick(); } },
      it.icon ? icon(it.icon) : null, it.label));
  }
  document.body.append(m);
  const r = anchor.getBoundingClientRect();
  const w = m.offsetWidth, h = m.offsetHeight;
  m.style.left = Math.max(8, Math.min(r.right - w, innerWidth - w - 8)) + "px";
  m.style.top = (r.bottom + 6 + h > innerHeight ? Math.max(8, r.top - h - 6) : r.bottom + 6) + "px";
  openMenuNode = m;
  const first = m.querySelector("button");
  if (first) first.focus();
}
document.addEventListener("mousedown", (e) => { if (openMenuNode && !openMenuNode.contains(e.target)) closeMenu(); });

// ============================================================ small pieces

function empty(iconName, title, text) {
  return el("div", { class: "empty-state" }, el("div", { class: "empty-icon" }, icon(iconName)), el("b", {}, title), text ? el("span", {}, text) : null);
}
function card(head, ...content) {
  return el("section", { class: "card" }, head, content);
}
function cardHead(title, sub, ...actions) {
  return el("div", { class: "card-head" }, el("div", {}, el("h3", {}, title), sub ? el("div", { class: "sub" }, sub) : null),
    actions.length ? el("div", { class: "row" }, actions) : null);
}
function kpi({ label, value, sub, iconName, tone = "", href }) {
  const k = el(href ? "a" : "div", { class: "card kpi" + (href ? " link" : ""), href: href || null },
    el("div", { class: "kpi-top" }, el("span", {}, label), iconName ? el("span", { class: ("kpi-icon " + tone).trim() }, icon(iconName)) : null),
    el("div", { class: "kpi-value" }, value),
    sub ? el("div", { class: "kpi-sub" }, sub) : null);
  if (href) k.style.textDecoration = "none";
  if (href) k.style.color = "inherit";
  return k;
}
function table(headers, rows, opts = {}) {
  return el("div", { class: "table-wrap" },
    el("table", { class: "data" + (opts.cls ? " " + opts.cls : "") },
      el("thead", {}, el("tr", {}, headers.map((h) => (typeof h === "string" ? el("th", {}, h) : el("th", { class: h.cls || "" }, h.label))))),
      el("tbody", {}, rows)));
}
function btn(label, opts = {}) {
  return el("button", { type: "button", class: "btn " + (opts.kind || ""), title: opts.title || null, disabled: opts.disabled || null,
    "aria-label": opts.iconOnly ? label : null, onclick: opts.onClick || null },
  opts.icon ? icon(opts.icon) : null, opts.iconOnly ? null : label);
}
/** A row of buttons, one of them on. `name` names the row for screen readers. */
function segmented(options, value, onChange, name) {
  const box = el("div", { class: "segmented", role: "tablist", "aria-label": name || null });
  for (const [v, label, count] of options) {
    box.append(el("button", { type: "button", role: "tab", class: v === value ? "on" : "", "aria-selected": String(v === value), onclick: () => onChange(v) },
      label, count !== undefined && count !== null ? el("span", { class: "count" }, fmtNum(count)) : null));
  }
  return box;
}
function debounce(fn, ms) {
  let h;
  return (...a) => { clearTimeout(h); h = setTimeout(() => fn(...a), ms); };
}
function copyText(text) {
  const done = () => toast(t("Copied."), "success");
  // Without the clipboard API (the portal on plain HTTP), the old way: a
  // hidden text area, selected and copied. Either way the user is told.
  const oldWay = () => {
    const back = document.activeElement;
    const area = el("textarea", { class: "copy-buffer", readonly: true, "aria-hidden": "true", tabindex: "-1" });
    area.value = text;
    document.body.append(area);
    area.select();
    let ok = false;
    try { ok = document.execCommand("copy"); } catch (_) { ok = false; }
    area.remove();
    if (back && back.focus && document.contains(back)) back.focus();
    if (ok) done(); else toast(t("Couldn't copy."), "error");
  };
  if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(done, oldWay);
  else oldWay();
}
/** Makes a clickable row reachable by keyboard: Tab to it, Enter or Space
 * does what a click does. */
function clickable(node, fn) {
  node.tabIndex = 0;
  node.addEventListener("click", fn);
  node.addEventListener("keydown", (e) => {
    if (e.target !== node || (e.key !== "Enter" && e.key !== " ")) return;
    e.preventDefault();
    fn(e);
  });
  return node;
}
function loadingPage() {
  return el("div", { class: "page-loading" },
    el("div", { class: "skeleton line" }), el("div", { class: "kpis" }, [0, 1, 2, 3].map(() => el("div", { class: "skeleton block" }))),
    el("div", { class: "skeleton block" }), el("div", { class: "skeleton block" }));
}
function updatedLabel(onRefresh, auto) {
  const label = el("span", {}, tf("Updated at {time}", { time: fmtClockSecs(new Date()) }));
  const again = onRefresh ? btn(t("Refresh"), { kind: "sm ghost icon-only", icon: "refresh", iconOnly: true, onClick: () => onRefresh() }) : null;
  // The page is rebuilt under the button: the keyboard stays on it.
  if (again) again.dataset.focus = "refresh";
  return el("span", { class: "updated" }, auto ? el("span", { class: "badge success dot" }, t("Live")) : null, label, again);
}

// ===================================================================== charts

function niceMax(v) {
  if (!(v > 0)) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  for (const k of [1, 2, 5, 10]) if (k * p >= v) return k * p;
  return 10 * p;
}
/** Bars (and optionally a line) over evenly spaced columns, in SVG, with a
 * tooltip per column. */
function chart({ labels, titles, bars, line, every = 6, height = 200, unit = "", width = 960 }) {
  const W = width, H = height, left = 46, right = 14, top = 12, bottom = every ? 24 : 8;
  const n = labels.length;
  const all = bars.values.concat(line ? line.values : []);
  const max = niceMax(Math.max(0, ...all));
  const fmt = bars.format || ((v) => fmtNum(Math.round(v * 10) / 10));
  const step = (W - left - right) / Math.max(1, n);
  const x = (k) => left + step * k + step / 2;
  const y = (v) => top + (H - top - bottom) * (1 - v / max);
  const g = svg("svg", { viewBox: `0 0 ${W} ${H}`, class: "chart", role: "img", "aria-label": bars.name + (line ? " · " + line.name : "") });
  for (const f of [0, 0.5, 1]) {
    const yy = y(max * f);
    g.append(svg("line", { x1: left, x2: W - right, y1: yy, y2: yy, class: "grid-line" }),
      svg("text", { x: left - 8, y: yy + 4, class: "axis", "text-anchor": "end" }, fmt(max * f)));
  }
  const bw = Math.max(1.5, step * 0.66);
  bars.values.forEach((v, k) => {
    if (v > 0) {
      const h = Math.max(1.5, y(0) - y(v));
      g.append(svg("rect", { x: x(k) - bw / 2, y: y(0) - h, width: bw, height: h, rx: Math.min(3, bw / 3), class: "col " + bars.cls }));
    }
  });
  if (line) {
    g.append(svg("polyline", { points: line.values.map((v, k) => x(k).toFixed(1) + "," + y(v).toFixed(1)).join(" "), class: "line " + line.cls }));
    line.values.forEach((v, k) => { if (v > 0) g.append(svg("circle", { cx: x(k), cy: y(v), r: 2.6, class: "dot " + line.cls })); });
  }
  if (every) labels.forEach((l, k) => { if (k % every === 0 || k === n - 1) g.append(svg("text", { x: x(k), y: H - 6, class: "axis", "text-anchor": "middle" }, l)); });
  labels.forEach((_, k) => {
    let tip = (titles[k] || labels[k]) + "\n" + bars.name + ": " + fmt(bars.values[k]) + unit;
    if (line) tip += "\n" + line.name + ": " + fmtNum(line.values[k]);
    g.append(svg("rect", { x: left + step * k, y: top, width: step, height: H - top - bottom, class: "hit" }, svg("title", {}, tip)));
  });
  const key = (name, cls, kind) => el("span", { class: "key" }, el("i", { class: kind + " " + cls }), name);
  return el("div", { class: "chart-wrap" }, el("div", { class: "legend" }, key(bars.name, bars.cls, "sw-bar"), line ? key(line.name, line.cls, "sw-line") : null), g);
}
function barList(rows, { max, tone = "", format = fmtNum } = {}) {
  const top = max || Math.max(1, ...rows.map((r) => r.value));
  return el("div", { class: "barlist" }, rows.map((r) => {
    const fill = el("div", { class: ("bl-fill " + (r.tone || tone)).trim() });
    fill.style.width = Math.max(r.value > 0 ? 1.5 : 0, (r.value / top) * 100) + "%";
    return el("div", { class: "bl-row" }, el("div", { class: "bl-label" }, r.label), el("div", { class: "bl-track" }, fill), el("div", { class: "bl-value" }, format(r.value)));
  }));
}

// ================================================================ the shell

const NAV = [
  { group: "Overview" },
  { id: "dashboard", label: "Dashboard", icon: "dashboard", key: "d" },
  { id: "live", label: "Live", icon: "activity", key: "l" },
  { group: "Moderation" },
  { id: "players", label: "Players", icon: "users", key: "p" },
  { id: "reports", label: "Reports", icon: "flag", key: "r", count: "reports" },
  { id: "chat", label: "Lounge chat", icon: "chat", key: "c" },
  { id: "fairplay", label: "Fair play", icon: "target", key: "f" },
  { group: "Community" },
  { id: "community", label: "Board & announcements", icon: "megaphone", key: "b" },
  { id: "events", label: "Events", icon: "calendar", key: "v" },
  { group: "Business" },
  { id: "economy", label: "Economy", icon: "coins", key: "e" },
  { id: "metrics", label: "Metrics", icon: "chart", key: "m" },
  { group: "Administration" },
  { id: "audit", label: "Audit log", icon: "audit", key: "a" },
  { id: "system", label: "Server", icon: "server", key: "s" },
];
const counts = { reports: 0, online: null, version: "", reachable: true };

function applyTheme() {
  document.documentElement.dataset.theme = theme;
  document.documentElement.lang = lang;
}

function renderSidebar() {
  const route = parseRoute();
  const side = $("#sidebar");
  const focused = side.contains(document.activeElement) ? document.activeElement.getAttribute("href") : null;
  setKids(side,
    el("a", { class: "brand", href: "#/dashboard", style: null }, brandMark(), el("div", {}, el("b", {}, "Snook Legend"), el("span", {}, t("Staff portal")))),
    NAV.map((n) => {
      if (n.group) return el("div", { class: "nav-group" }, t(n.group));
      const c = n.count ? counts[n.count] : 0;
      return el("a", { class: "nav-link" + (route.page === n.id ? " on" : ""), href: "#/" + n.id, "aria-current": route.page === n.id ? "page" : null },
        icon(n.icon), el("span", { class: "grow ellipsis" }, t(n.label)),
        c ? el("span", { class: "nav-count", title: tn(c, "{n} open report", "{n} open reports") }, c > 99 ? "99+" : fmtNum(c)) : null);
    }),
    el("div", { class: "sidebar-foot" },
      el("span", { class: "live-dot" + (counts.reachable ? "" : " off") },
        counts.reachable ? (counts.online === null ? t("Connected") : tn(counts.online, "{n} player online", "{n} players online")) : t("Server unreachable")),
      counts.version ? el("span", {}, tf("Server {version}", { version: counts.version })) : null));
  const brand = $(".brand", side);
  brand.style.textDecoration = "none";
  brand.style.color = "inherit";
  // Rebuilt every minute (the counts): the keyboard stays on its link.
  if (focused) {
    const again = [...side.querySelectorAll("a")].find((a) => a.getAttribute("href") === focused);
    if (again) again.focus({ preventScroll: true });
  }
}

function renderTopbar() {
  const bar = $("#topbar");
  const menuBtn = btn(t("Menu"), { kind: "ghost icon-only menu-btn", icon: "menu", iconOnly: true, onClick: () => $("#app").classList.toggle("nav-open") });
  const langSel = el("select", { class: "select lang-select hide-sm", "aria-label": t("Language") },
    LANGS.map(([c, name]) => el("option", { value: c }, name)));
  langSel.value = lang;
  langSel.addEventListener("change", () => setLang(langSel.value));
  const themeBtn = btn(theme === "dark" ? t("Light theme") : t("Dark theme"), {
    kind: "ghost icon-only", icon: theme === "dark" ? "sun" : "moon", iconOnly: true,
    onClick: () => { theme = theme === "dark" ? "light" : "dark"; store.set(KEYS.theme, theme); applyTheme(); renderTopbar(); },
  });
  themeBtn.title = themeBtn.getAttribute("aria-label");
  const helpBtn = btn(t("Keyboard shortcuts"), { kind: "ghost icon-only hide-sm", icon: "keyboard", iconOnly: true, onClick: showShortcuts });
  helpBtn.title = t("Keyboard shortcuts") + " (?)";
  const meBtn = el("button", { class: "btn ghost me-chip", type: "button", "aria-label": t("Account") },
    avatar(me.username, "sm"),
    el("span", { class: "who" }, el("b", {}, me.username), el("span", { class: me.role === "admin" ? "admin" : "" }, roleLabel(me.role))),
    icon("chevronDown"));
  meBtn.addEventListener("click", () => openMenu(meBtn, [
    { group: me.username + " · " + roleLabel(me.role) },
    { label: t("My player page"), icon: "user", onClick: () => go("players/" + encodeURIComponent(me.username)) },
    { label: theme === "dark" ? t("Light theme") : t("Dark theme"), icon: theme === "dark" ? "sun" : "moon", onClick: () => themeBtn.click() },
    { group: t("Language") },
    ...LANGS.map(([c, name]) => ({ label: name + (c === lang ? " ✓" : ""), icon: "globe", onClick: () => setLang(c) })),
    "-",
    { label: t("Sign out"), icon: "logout", danger: true, onClick: () => signOut(true) },
  ]));
  setKids(bar, menuBtn, searchBox(), el("div", { class: "topbar-actions" }, langSel, themeBtn, helpBtn, meBtn));
}

function setLang(code) {
  lang = code;
  store.set(KEYS.lang, code);
  applyTheme();
  if (me) { renderTopbar(); renderSidebar(); render(); } else showLogin();
}

/** Players by name as you type; "#123" opens game 123 (only with "#": a
 * username may be all digits). */
function searchBox() {
  const input = el("input", { class: "input", type: "search", placeholder: t("Search players, or #game number"), "aria-label": t("Search"), autocomplete: "off", spellcheck: "false" });
  const results = el("div", { class: "search-results hidden", role: "listbox" });
  const wrap = el("div", { class: "search" }, icon("search", "lead"), input, el("kbd", { "aria-hidden": "true" }, "/"), results);
  let sel = -1, items = [], seq = 0;
  const close = () => { results.classList.add("hidden"); sel = -1; };
  const choose = (k) => { const it = items[k]; if (it) { close(); input.value = ""; input.blur(); it.go(); } };
  const draw = () => {
    setKids(results, ...items.map((it, k) => {
      const b = el("button", { type: "button", class: "result" + (k === sel ? " sel" : ""), role: "option", onmousedown: (e) => { e.preventDefault(); choose(k); } }, it.node);
      return b;
    }));
    if (!items.length) results.append(el("div", { class: "hint" }, t("No player by that name.")));
    results.classList.remove("hidden");
  };
  const lookup = debounce(async () => {
    const q = input.value.trim();
    const mine = ++seq;
    if (!q) { close(); return; }
    const game = q.match(/^#(\d{1,18})$/);
    items = [];
    // A stored game, or else a game being played (Live shows their numbers).
    if (game) items.push({ node: el("span", { class: "row" }, icon("play"), tf("Watch game #{id}", { id: game[1] })), go: () => openViewer("/replay/" + game[1] + "/view", "/live/" + game[1] + "/view") });
    try {
      const d = await api("/users?q=" + encodeURIComponent(q.replace(/^#/, "")) + "&sort=last_seen");
      if (mine !== seq) return;
      for (const u of d.users.slice(0, 7)) {
        items.push({
          node: el("span", { class: "person grow" }, avatar(u.username, "sm"), el("span", { class: "person-name" }, el("b", {}, u.username), userBadges(u)), el("span", { class: "meta" }, ratingPair(u))),
          go: () => go("players/" + encodeURIComponent(u.username)),
        });
      }
      if (d.total > 7) items.push({ node: el("span", { class: "muted" }, tf("All {n} results", { n: fmtNum(d.total) })), go: () => go("players?q=" + encodeURIComponent(q)) });
    } catch (_) { /* the list stays as it is */ }
    sel = items.length ? 0 : -1;
    draw();
  }, 220);
  input.addEventListener("input", lookup);
  input.addEventListener("focus", () => { if (input.value.trim()) lookup(); });
  input.addEventListener("blur", () => setTimeout(close, 120));
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); sel = Math.min(items.length - 1, sel + 1); draw(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); sel = Math.max(0, sel - 1); draw(); }
    else if (e.key === "Enter") { e.preventDefault(); if (sel >= 0) choose(sel); else if (input.value.trim()) { go("players?q=" + encodeURIComponent(input.value.trim())); close(); } }
    else if (e.key === "Escape") { close(); input.blur(); }
  });
  return wrap;
}

// ==================================================================== router

function parseRoute() {
  const raw = (location.hash || "#/dashboard").replace(/^#\/?/, "");
  const [path, qs] = raw.split("?");
  const parts = path.split("/").filter(Boolean).map((p) => { try { return decodeURIComponent(p); } catch (_) { return p; } });
  return { page: parts[0] || "dashboard", parts: parts.slice(1), query: new URLSearchParams(qs || "") };
}
function go(path) {
  const target = "#/" + path;
  if (location.hash === target) render();
  else location.hash = target;
}
/** Changes the query of the current page without a new render. */
function setQuery(params) {
  const r = parseRoute();
  const q = new URLSearchParams(r.query);
  for (const [k, v] of Object.entries(params)) { if (v === "" || v === null || v === undefined) q.delete(k); else q.set(k, v); }
  const path = [r.page, ...r.parts.map(encodeURIComponent)].join("/");
  const s = q.toString();
  history.replaceState(null, "", "#/" + path + (s ? "?" + s : ""));
}

let page = { cleanup: [] };
function onLeave(fn) { page.cleanup.push(fn); }
/** Runs `fn` every `ms` while this page is shown and the tab is visible,
 * but not while the user is busy on it: a dialog, a menu or the replay
 * open, or a field being typed in. */
function every(ms, fn) {
  const h = setInterval(() => {
    if (document.visibilityState !== "visible" || openDialogs.length || openMenuNode || viewer) return;
    const a = document.activeElement;
    if (isTextField(a) || (a && a.tagName === "SELECT")) return;
    fn();
  }, ms);
  onLeave(() => clearInterval(h));
}

const PAGES = {};
let renderSeq = 0;
/** Draws the page in the address. A quiet render (refreshes, searching as
 * you type) leaves the page on screen until the new one is ready, with no
 * skeleton, and carries over what the user was doing (see swapPage). */
async function render(opts = {}) {
  if (!me) return;
  const quiet = !!opts.quiet;
  for (const f of page.cleanup) { try { f(); } catch (_) { /* ignore */ } }
  page = { cleanup: [] };
  if (!quiet) {
    closeMenu();
    $("#app").classList.remove("nav-open");
  }
  const route = parseRoute();
  renderSidebar();
  const main = $("#main");
  const fn = PAGES[route.page];
  const seq = ++renderSeq;
  if (!fn) { setKids(main, empty("info", t("Nothing here"), t("That page doesn't exist."))); return; }
  if (!quiet) setKids(main, loadingPage());
  try {
    const node = await fn(route);
    if (seq !== renderSeq) return;
    swapPage(main, node, quiet);
    const label = NAV.find((n) => n.id === route.page);
    document.title = (label ? t(label.label) + " · " : "") + "Snook Legend · " + t("Staff");
  } catch (e) {
    if (seq !== renderSeq) return;
    if (e.status === 401) return;
    setKids(main, el("div", { class: "error-page" },
      el("div", { class: "empty-icon" }, icon("warn")), el("b", {}, t("This page couldn't be loaded.")),
      el("span", { class: "muted" }, e.message), btn(t("Try again"), { kind: "primary", icon: "refresh", onClick: () => render() })));
  }
}
/** Puts the new page in place of the old one. A quiet swap keeps the scroll
 * position, the text of fields marked data-keep (drafts), and the field the
 * user is in, marked data-focus, with what they typed and the caret. */
function swapPage(main, node, quiet) {
  closeMenu();
  if (!quiet) { setKids(main, node); return; }
  const scroll = window.scrollY;
  const active = document.activeElement;
  const hadFocus = !!active && active !== main && main.contains(active);
  const text = hadFocus && isTextField(active);
  const typing = hadFocus && active.dataset.focus ? {
    key: active.dataset.focus, value: text ? active.value : null,
    start: text ? active.selectionStart : null, end: text ? active.selectionEnd : null,
  } : null;
  const drafts = new Map([...main.querySelectorAll("[data-keep]")].map((x) => [x.dataset.keep, x.value]));
  setKids(main, node);
  for (const x of main.querySelectorAll("[data-keep]")) {
    const v = drafts.get(x.dataset.keep);
    if (v !== undefined && v !== x.value) { x.value = v; x.dispatchEvent(new Event("input")); }
  }
  window.scrollTo(window.scrollX, scroll);
  if (!hadFocus) return;
  const again = typing ? [...main.querySelectorAll("[data-focus]")].find((x) => x.dataset.focus === typing.key) : null;
  // Nothing to go back to: the keyboard stays in the page, not at its top.
  if (!again) { main.focus({ preventScroll: true }); return; }
  if (typing.value !== null && isTextField(again)) again.value = typing.value;
  again.focus({ preventScroll: true });
  if (typing.start !== null && isTextField(again)) {
    try { again.setSelectionRange(typing.start, typing.end); } catch (_) { /* a field without a caret */ }
  }
}
/** Draws the page again, quietly (the refresh buttons). */
const refreshPage = () => render({ quiet: true });
/** Re-renders the page, quietly, if it is still the one shown. */
function refreshIf(pageId) { if (parseRoute().page === pageId) render({ quiet: true }); }

function pageHead(title, sub, ...actions) {
  return el("div", { class: "page-head" },
    el("div", {}, el("h1", {}, title), sub ? el("div", { class: "sub" }, sub) : null),
    actions.length ? el("div", { class: "head-actions" }, actions) : null);
}

// ================================================================= shortcuts

let pendingG = 0;
document.addEventListener("keydown", (e) => {
  trapFocus(e);
  if (e.key === "Escape") {
    if (openMenuNode) { closeMenu(); return; }
    if (viewer) { closeViewer(); return; }
    const top = openDialogs[openDialogs.length - 1];
    if (top) { top.close(false); return; }
  }
  // The replay keeps the keys, even while it loads.
  if (viewer) { if (viewer.keys) viewer.keys(e); return; }
  if (!me || openDialogs.length) return;
  const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement && document.activeElement.tagName);
  if (typing || e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.key === "/") { e.preventDefault(); const s = $("#topbar .search input"); if (s) s.focus(); return; }
  if (e.key === "?") { e.preventDefault(); showShortcuts(); return; }
  if (e.key === "g") { pendingG = Date.now(); return; }
  if (Date.now() - pendingG < 1200) {
    const n = NAV.find((x) => x.key === e.key.toLowerCase());
    pendingG = 0;
    if (n) { e.preventDefault(); go(n.id); }
  }
});
function showShortcuts() {
  const row = (label, ...keys) => el("div", { class: "sc" }, el("span", {}, label), el("span", {}, keys.map((k) => el("kbd", {}, k))));
  dialog({
    title: t("Keyboard shortcuts"), iconName: "keyboard",
    body: [el("div", { class: "shortcuts" },
      row(t("Search players"), "/"),
      row(t("This list"), "?"),
      row(t("Close a dialog or the replay"), "Esc"),
      ...NAV.filter((n) => n.key).map((n) => row(tf("Go to {page}", { page: t(n.label) }), "g", n.key)),
      row(t("Replay: play or pause"), t("Space")),
      row(t("Replay: previous or next stroke"), "←", "→"),
      row(t("Replay: one frame back or forward"), ",", "."))],
    actions: [{ label: t("Close"), kind: "primary", primary: true, onClick: (d) => d.close() }],
  });
}

// ============================================================ signing in

function showLogin(message) {
  $("#boot").classList.add("hidden");
  $("#app").classList.add("hidden");
  const box = $("#login");
  box.classList.remove("hidden");
  const user = el("input", { class: "input", id: "login-user", autocomplete: "username", required: true, spellcheck: "false" });
  const pass = el("input", { class: "input", id: "login-pass", type: "password", autocomplete: "current-password", required: true });
  const show = btn(t("Show password"), { kind: "ghost icon-only", icon: "eye", iconOnly: true });
  show.addEventListener("click", () => {
    const on = pass.type === "password";
    pass.type = on ? "text" : "password";
    setKids(show, icon(on ? "eyeOff" : "eye"));
    show.setAttribute("aria-label", on ? t("Hide password") : t("Show password"));
  });
  const alert = el("div", { class: "alert danger" + (message ? "" : " hidden"), role: "alert" }, icon("warn"), el("div", {}, message || ""));
  const submit = el("button", { class: "btn primary", type: "submit" }, t("Sign in"));
  const langSel = el("select", { class: "select", "aria-label": t("Language") }, LANGS.map(([c, n]) => el("option", { value: c }, n)));
  langSel.value = lang;
  langSel.addEventListener("change", () => setLang(langSel.value));
  const form = el("form", { class: "login-card", novalidate: true },
    el("div", { class: "login-brand" }, brandMark(), el("div", {}, el("h1", {}, "Snook Legend"), el("p", {}, t("Staff portal")))),
    el("p", { class: "muted" }, t("Sign in with your game account. Only moderators and administrators have access.")),
    alert,
    el("div", { class: "field" }, el("label", { class: "label", for: "login-user" }, t("Username")), user),
    el("div", { class: "field" }, el("label", { class: "label", for: "login-pass" }, t("Password")), el("div", { class: "pass-wrap" }, pass, show)),
    submit,
    el("div", { class: "login-foot" }, el("span", {}, el("span", { class: "row" }, icon("lock"), t("Sessions end after 8 hours."))), langSel));
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    alert.classList.add("hidden");
    if (!user.value.trim() || !pass.value) {
      setKids(alert, icon("warn"), el("div", {}, t("Enter your username and password.")));
      alert.classList.remove("hidden");
      return;
    }
    submit.disabled = true;
    setKids(submit, el("span", { class: "spinner" }), t("Signing in…"));
    try {
      const d = await api("/login", { method: "POST", body: { username: user.value.trim(), password: pass.value } });
      token = d.token;
      try { sessionStorage.setItem(KEYS.token, token); } catch (_) { /* private mode */ }
      pass.value = "";
      await start();
    } catch (err) {
      setKids(alert, icon("warn"), el("div", {}, err.message));
      alert.classList.remove("hidden");
      pass.select();
    } finally {
      submit.disabled = false;
      submit.textContent = t("Sign in");
    }
  });
  setKids(box, form);
  setTimeout(() => (user.value ? pass : user).focus(), 30);
}

function sessionExpired() {
  if (!me) return;
  token = "";
  me = null;
  try { sessionStorage.removeItem(KEYS.token); } catch (_) { /* ignore */ }
  for (const d of [...openDialogs]) d.close(false);
  closeViewer();
  showLogin(t("Your session expired. Sign in again."));
}

function signOut(callServer = true) {
  if (callServer && token) api("/logout", { method: "POST" }).catch(() => {});
  token = "";
  me = null;
  try { sessionStorage.removeItem(KEYS.token); } catch (_) { /* ignore */ }
  for (const d of [...openDialogs]) d.close(false);
  closeViewer();
  showLogin();
}

let pollTimer = 0;
async function pollCounts() {
  if (!me) return;
  try {
    const o = await api("/overview");
    counts.reports = o.stored.open_reports || 0;
    counts.online = o.online;
    counts.version = o.version;
    counts.reachable = true;
  } catch (e) {
    if (e.status === 0) counts.reachable = false;
  }
  if (me) renderSidebar();
}

async function start() {
  applyTheme();
  if (!token) { showLogin(); return; }
  try {
    me = await api("/me");
  } catch (e) {
    me = null;
    showLogin(e.status === 401 ? null : e.message);
    return;
  }
  $("#boot").classList.add("hidden");
  $("#login").classList.add("hidden");
  $("#app").classList.remove("hidden");
  renderTopbar();
  render();
  pollCounts();
  clearInterval(pollTimer);
  pollTimer = setInterval(() => { if (document.visibilityState === "visible") pollCounts(); }, 60000);
}

window.addEventListener("hashchange", () => render());
$("#scrim").addEventListener("click", () => $("#app").classList.remove("nav-open"));

// ================================================================= dashboard

PAGES.dashboard = async () => {
  const [o, m, rep, aud] = await Promise.all([
    api("/overview"), api("/metrics"), api("/reports?status=open"), api("/audit"),
  ]);
  counts.reports = o.stored.open_reports || 0;
  const hour = new Date().getHours();
  const hello = hour < 12 ? t("Good morning, {name}") : hour < 18 ? t("Good afternoon, {name}") : t("Good evening, {name}");
  const today = new Intl.DateTimeFormat(lang, { weekday: "long", day: "numeric", month: "long" }).format(new Date());
  const alerts = [];
  if (!o.persistent) alerts.push(alertBox("danger", t("Nothing is saved"), t("This server keeps everything in memory: accounts and games are lost when it stops.")));
  if (m.draining) alerts.push(alertBox("warning", t("The server is restarting"), t("No new games start; the games in progress move to the next server.")));
  if (o.online >= 4 && o.addresses === 1) alerts.push(alertBox("warning", t("Every player comes from one address"), t("Set SNOOK_LEGEND_CLIENT_IP_HEADER (see DEPLOY.md), or the limits per address will turn players away.")));
  if (counts.reports > 0) alerts.push(alertBox("info", tn(counts.reports, "{n} report waiting", "{n} reports waiting"), t("Players are waiting for an answer."), btn(t("Review"), { kind: "sm", onClick: () => go("reports") })));

  const hours = m.metrics.hours || [];
  const label = (h) => String(new Date(h * 3600000).getHours()).padStart(2, "0") + "h";
  const title = (h) => new Intl.DateTimeFormat(lang, { weekday: "short", hour: "2-digit", minute: "2-digit" }).format(new Date(h * 3600000));
  const activity = hours.some((h) => h.active > 0 || h.peak_players > 0)
    ? chart({ labels: hours.map((h) => label(h.hour)), titles: hours.map((h) => title(h.hour)),
      bars: { values: hours.map((h) => h.active), name: t("Players active"), cls: "c-accent" },
      line: { values: hours.map((h) => h.peak_players), name: t("Most online at once"), cls: "c-info" }, every: 6, height: 210 })
    : empty("chart", t("No activity yet"), t("The server counts who is online every minute: the first hour shows up shortly."));

  const reports = rep.reports.slice(0, 6);
  const recentAudit = aud.entries.filter((e) => e.action !== "login").slice(0, 8);
  const revenue = (m.metrics.revenue || []).filter((r) => r[0] !== "msstore");
  return el("div", {},
    pageHead(tf(hello, { name: me.username }), today.charAt(0).toUpperCase() + today.slice(1), updatedLabel(refreshPage)),
    alerts.length ? el("div", { class: "stack" }, alerts) : null,
    el("div", { class: "kpis section" },
      kpi({ label: t("Online now"), value: fmtNum(o.online), sub: perRulesText(o.online_by_rules), iconName: "users", tone: "success", href: "#/live" }),
      kpi({ label: t("Games now"), value: fmtNum(o.matches_live), sub: tn(o.tables_open, "{n} open table", "{n} open tables"), iconName: "activity", tone: "info", href: "#/live" }),
      kpi({ label: t("In the queue"), value: fmtNum(m.queue.size), sub: tf("median wait {s} s", { s: fmtNum(m.queue.median_wait_secs) }), iconName: "queue" }),
      kpi({ label: t("Open reports"), value: fmtNum(o.stored.open_reports), sub: o.stored.open_reports ? t("waiting for review") : t("all caught up"), iconName: "flag", tone: o.stored.open_reports ? "danger" : "success", href: "#/reports" }),
      kpi({ label: t("Accounts"), value: fmtNum(o.stored.users), sub: tf("+{n} in 7 days", { n: fmtNum(o.stored.new_users_7d) }), iconName: "user", href: "#/players" }),
      kpi({ label: t("Active in 7 days"), value: fmtNum(o.stored.active_users_7d), sub: o.stored.users ? fmtPct(o.stored.active_users_7d * 100 / o.stored.users) + " " + t("of the accounts") : null, iconName: "activity", tone: "success", href: "#/metrics" }),
      kpi({ label: t("Games in 24 h"), value: fmtNum(o.stored.matches_24h), sub: perRulesText(o.stored.matches_24h_by_rules) + " · " + tf("{n} in all", { n: fmtNum(o.stored.matches_total) }), iconName: "trophy", href: "#/metrics" }),
      kpi({ label: t("Revenue in 30 days"), value: revenue.length ? money(revenue[0][1], revenue[0][0]) : "—", sub: revenue.length > 1 ? tf("and {n} other currencies", { n: revenue.length - 1 }) : revenue.length ? tn(revenue[0][2], "{n} payment", "{n} payments") : t("no payments"), iconName: "coins", tone: "warning", href: "#/economy" })),
    el("div", { class: "grid-aside section" },
      el("div", { class: "stack" },
        card(cardHead(t("Activity in the last 48 hours"), t("Players active each hour and the most online at once")), el("div", { class: "card-body" }, activity)),
        card(cardHead(t("Staff activity"), null, btn(t("Audit log"), { kind: "sm ghost", onClick: () => go("audit") })),
          recentAudit.length ? el("div", { class: "mini-list" }, recentAudit.map(auditMini)) : empty("audit", t("Nothing yet"), t("Every staff action is written here.")))),
      el("div", { class: "stack" },
        card(cardHead(t("Open reports"), null, btn(t("See all"), { kind: "sm ghost", onClick: () => go("reports") })),
          reports.length ? el("div", { class: "mini-list" }, reports.map((r) => clickable(el("div", { class: "mi click" },
            reasonBadge(r.reason), el("div", { class: "mi-main" }, el("b", {}, r.target), el("span", { class: "mi-sub" }, tf("by {name}", { name: r.reporter }))),
            el("span", { class: "mi-side" }, timeEl(r.created_at))), () => go("reports"))))
            : empty("checkCircle", t("No open reports"), t("Nothing waits for the staff."))),
        card(cardHead(t("The server")), el("div", { class: "card-body health" },
          healthRow(o.persistent, o.persistent ? t("Saved in the database") : t("Kept in memory only")),
          healthRow(!m.draining, m.draining ? t("Restarting: no new games") : t("Taking new games")),
          healthRow(true, tf("Up for {time}", { time: fmtDuration(o.uptime_secs) }), "v" + o.version),
          healthRow(true, tn(o.addresses, "Players online from {n} address", "Players online from {n} addresses")),
          o.motd ? el("div", { class: "preview-box" }, el("span", { class: "faint" }, t("Welcome message")), o.motd) : null)))));
};
function alertBox(tone, title, text, action) {
  return el("div", { class: "alert " + tone }, icon(tone === "danger" || tone === "warning" ? "warn" : "info"),
    el("div", { class: "alert-body grow" }, el("b", {}, title), text ? el("span", {}, text) : null), action || null);
}
function healthRow(ok, text, side, warnOnly) {
  return el("div", { class: "h-row " + (ok ? "ok" : warnOnly ? "warn" : "bad") }, icon(ok ? "checkCircle" : "warn"), el("span", {}, text), side ? el("span", { class: "h-sub" }, side) : null);
}

// =================================================================== players

const PLAYER_FILTERS = [["all", "All"], ["online", "Online"], ["staff", "Staff"], ["banned", "Banned"], ["muted", "Muted"], ["deleted", "Deleted"]];
const PLAYER_SORTS = [["newest", "Newest accounts"], ["last_seen", "Seen most recently"], ["rating-brazilian", "Highest rating, Brazilian rules"], ["rating-official", "Highest rating, official rules"], ["name", "Name"]];

PAGES.players = async (route) => {
  if (route.parts.length) return playerPage(route.parts[0], route.parts[1] || "overview");
  const q = route.query.get("q") || "";
  const filter = route.query.get("filter") || "all";
  const sort = route.query.get("sort") || "newest";
  const pageNo = Math.max(0, Number(route.query.get("page")) || 0);
  const d = await api("/users?" + new URLSearchParams({ q, filter, sort, page: pageNo }));
  const search = el("input", { class: "input wide", type: "search", placeholder: t("Name, or a recovery e-mail"), value: q, "aria-label": t("Search players"), "data-focus": "players-q" });
  const apply = (changes, quiet) => { setQuery({ q: search.value.trim(), filter, sort, page: 0, ...changes }); render({ quiet }); };
  // Quietly, so the field keeps the keyboard while the list follows.
  search.addEventListener("input", debounce(() => apply({}, true), 350));
  const sortSel = el("select", { class: "select", "aria-label": t("Order"), "data-focus": "players-sort" }, PLAYER_SORTS.map(([v, l]) => el("option", { value: v }, t(l))));
  sortSel.value = sort;
  sortSel.addEventListener("change", () => apply({ sort: sortSel.value }));
  const pages = Math.max(1, Math.ceil(d.total / d.per_page));
  const from = d.total ? pageNo * d.per_page + 1 : 0, to = Math.min(d.total, (pageNo + 1) * d.per_page);
  // A player's rating under one of the rules: the tier and the record
  // under it (faint when they never played those rules rated).
  const ratingCell = (u, r) => {
    const x = recordOf(u, r);
    return el("td", { class: "num rating-cell" + (x.games ? "" : " unplayed"), title: tf("{wins} wins · {rate}", { wins: fmtNum(x.wins), rate: winRate(x) }) },
      el("b", {}, fmtNum(x.rating)),
      el("div", { class: "faint" }, x.games ? tierLabel(x.tier) + " · " + tn(x.games, "{n} game", "{n} games") : t("not played yet")));
  };
  const rows = d.users.map((u) => clickable(el("tr", { class: "click" },
    el("td", {}, el("span", { class: "person" }, avatar(u.username), el("span", { class: "person-name" }, el("b", { class: "name" }, u.username), userBadges(u, { online: false })))),
    el("td", {}, country(u.country)),
    ratingCell(u, "brazilian"),
    ratingCell(u, "official"),
    el("td", { class: "nowrap" }, timeEl(u.created_at)),
    el("td", { class: "nowrap" }, u.online ? badge(t("Online"), "success dot") : timeEl(u.last_seen))), () => go("players/" + encodeURIComponent(u.username))));
  const rulesHead = (r) => ({ label: el("span", { class: "th-rules" }, rulesMark(r), rulesShort(r)), cls: "num" });
  const goPage = (p) => { setQuery({ page: p }); render(); };
  return el("div", {},
    pageHead(t("Players"), tn(d.total, "{n} account", "{n} accounts")),
    el("div", { class: "toolbar" }, search,
      segmented(PLAYER_FILTERS.map(([v, l]) => [v, t(l)]), filter, (v) => apply({ filter: v }), t("Filter")), sortSel),
    card(null,
      rows.length ? table([t("Player"), t("Country"), rulesHead("brazilian"), rulesHead("official"), t("Joined"), t("Last seen")], rows)
        : empty("users", t("No players found"), q ? t("Try another name, or another filter.") : t("Nobody matches this filter.")),
      d.total > d.per_page ? el("div", { class: "pager" },
        el("span", {}, tf("{from}–{to} of {total}", { from: fmtNum(from), to: fmtNum(to), total: fmtNum(d.total) })),
        el("div", { class: "row" },
          btn(t("Previous"), { kind: "sm", icon: "chevronLeft", disabled: pageNo === 0, onClick: () => goPage(pageNo - 1) }),
          el("span", { class: "muted" }, tf("Page {n} of {total}", { n: pageNo + 1, total: pages })),
          btn(t("Next"), { kind: "sm", disabled: pageNo + 1 >= pages, onClick: () => goPage(pageNo + 1) }))) : null));
};

// ------------------------------------------------------------ one player

const PLAYER_TABS = [["overview", "Overview", "user"], ["moderation", "Moderation", "shield"], ["games", "Games", "trophy"], ["computer", "Against the computer", "target"], ["economy", "Economy", "coins"], ["achievements", "Achievements", "star"]];

async function playerPage(name, tab) {
  const d = await api("/users/" + encodeURIComponent(name));
  const u = d.user;
  const now = nowSecs();
  const banned = u.banned_until && u.banned_until > now;
  const muted = u.muted_until && u.muted_until > now;
  const path = "players/" + encodeURIComponent(u.username);
  const reportsOpen = d.reports.filter((r) => r.status === 0).length;
  const moreBtn = btn(t("More"), { icon: "more" });
  moreBtn.addEventListener("click", () => openMenu(moreBtn, [
    { label: t("Copy the name"), icon: "copy", onClick: () => copyText(u.username) },
    { label: t("Add a staff note"), icon: "note", onClick: () => noteDialog(u, "note") },
    u.status_message ? { label: t("Clear the status message"), icon: "trash", onClick: () => playerAction(u, "clear-status", {}, t("Status message cleared.")) } : null,
    { label: t("Sign out everywhere"), icon: "logout", onClick: async () => {
      if (await confirmDialog({ title: tf("Sign {name} out everywhere?", { name: u.username }), text: t("Every session of the account ends; the player signs in again."), confirm: t("Sign out"), tone: "warning" })) {
        playerAction(u, "logout-all", {}, t("Signed out everywhere."));
      }
    } },
    isAdmin() ? "-" : null,
    isAdmin() ? { group: t("Administrators") } : null,
    isAdmin() ? { label: t("Rename…"), icon: "edit", onClick: () => renameDialog(u) } : null,
    isAdmin() ? { label: t("Change the role…"), icon: "shield", onClick: () => roleDialog(u) } : null,
    isAdmin() ? { label: t("Set a new password…"), icon: "key", onClick: () => passwordDialog(u) } : null,
    isAdmin() ? { label: t("Give coins…"), icon: "coins", onClick: () => coinsDialog(u) } : null,
    isAdmin() ? { label: t("Give an item…"), icon: "gift", onClick: () => itemDialog(u) } : null,
    isAdmin() ? { label: t("Give VIP days…"), icon: "star", onClick: () => vipDialog(u) } : null,
  ]));
  const isSelf = me.username.toLowerCase() === u.username.toLowerCase();
  const actions = u.deleted_at ? [] : [
    isSelf ? null : btn(t("Warn"), { icon: "warn", onClick: () => noteDialog(u, "warning") }),
    isSelf ? null : muted ? btn(t("Unmute"), { icon: "undo", onClick: () => playerAction(u, "unmute", {}, t("Unmuted.")) }) : btn(t("Mute"), { icon: "mute", onClick: () => muteDialog(u) }),
    !isSelf && u.online ? btn(t("Disconnect"), { icon: "kick", onClick: () => kickDialog(u) }) : null,
    isSelf ? null : banned ? btn(t("Lift the ban"), { kind: "", icon: "undo", onClick: () => unbanDialog(u) }) : btn(t("Ban"), { kind: "danger-soft", icon: "ban", onClick: () => banDialog(u) }),
    moreBtn,
  ];
  const tabs = el("div", { class: "tabs", role: "tablist" }, PLAYER_TABS.map(([id, label, ic]) => {
    const n = id === "moderation" ? d.notes.length + d.reports.length : id === "games" ? d.history.length : id === "achievements" ? d.achievements.length : null;
    return el("a", { class: "tab" + (tab === id ? " on" : ""), role: "tab", "aria-selected": String(tab === id), href: "#/" + path + (id === "overview" ? "" : "/" + id) },
      icon(ic), t(label), n ? el("span", { class: "count" + (id === "moderation" && reportsOpen ? " alert" : "") }, fmtNum(n)) : null);
  }));
  tabs.querySelectorAll(".tab").forEach((a) => { a.style.textDecoration = "none"; });
  const body = { overview: playerOverview, moderation: playerModeration, games: playerGames, computer: playerComputer, economy: playerEconomy, achievements: playerAchievements }[tab] || playerOverview;
  return el("div", {},
    el("div", { class: "crumbs" }, el("a", { href: "#/players" }, t("Players")), icon("chevronRight"), el("span", {}, u.username)),
    card(null, el("div", { class: "card-body profile-head" },
      avatar(u.username, "lg"),
      el("div", { class: "who" },
        el("h1", {}, u.username, userBadges(u)),
        el("div", { class: "facts" },
          el("span", {}, country(u.country)),
          RULES.map((r) => el("span", { class: "fact-rating" }, rulesBadge(r), fmtNum(recordOf(u, r).rating) + " · " + tierLabel(recordOf(u, r).tier))),
          el("span", {}, icon("calendar"), tf("joined {when}", { when: fmtDateOnly(u.created_at) })),
          el("span", {}, icon("clock"), u.online ? t("online now") : tf("seen {when}", { when: ago(u.last_seen) })),
          el("span", { class: "faint" }, "ID " + u.id))),
      el("div", { class: "action-bar" }, actions))),
    banned || muted || u.deleted_at ? el("div", { class: "stack section" },
      u.deleted_at ? alertBox("info", t("Account deleted"), tf("Deleted and anonymised on {when}.", { when: fmtDate(u.deleted_at) })) : null,
      banned ? alertBox("danger", u.banned_until >= FOREVER ? t("Banned for good") : tf("Banned until {when}", { when: fmtDate(u.banned_until) }), u.ban_reason ? tf("Reason: {reason}", { reason: u.ban_reason }) : null) : null,
      muted ? alertBox("warning", tf("Muted until {when}", { when: fmtDate(u.muted_until) }), t("They can play but not write in the chat.")) : null) : null,
    el("div", { class: "section" }, tabs, body(d)));
}

/** Where the server leaves out what only administrators see (e-mails,
 * payments): said so, instead of "none". */
const adminOnly = () => el("span", { class: "faint" }, t("Visible to administrators only"));

/** A player's standing under one of the rules: the rating and its tier,
 * the peak, and the record. `now` marks the rules they play right now. */
function rulesStanding(u, r, now) {
  const x = recordOf(u, r);
  return el("section", { class: "card rules-card " + r + (x.games ? "" : " unplayed") },
    el("div", { class: "rc-head" },
      el("span", { class: "rc-title" }, rulesMark(r), rulesName(r)),
      now ? badge(t("Playing these now"), "success dot") : null),
    el("div", { class: "rc-main" },
      el("span", { class: "rc-rating" }, fmtNum(x.rating)),
      el("span", { class: "rc-tier" }, tierLabel(x.tier)),
      el("span", { class: "rc-peak faint" }, tf("peak {n}", { n: fmtNum(x.peak_rating) }))),
    x.games ? el("div", { class: "rc-stats" },
      el("span", {}, el("b", {}, fmtNum(x.games)), " ", tn(x.games, "rated game", "rated games")),
      el("span", {}, el("b", {}, fmtNum(x.wins)), " ", tn(x.wins, "win", "wins")),
      el("span", {}, el("b", {}, winRate(x, 1)), " ", t("won")))
      : el("div", { class: "rc-stats faint" }, t("No rated game under these rules yet.")));
}

function playerOverview(d) {
  const u = d.user, c = d.career || {};
  const pot = c.shots ? fmtPct(c.potting_shots * 100 / c.shots) : "—";
  const presence = d.presence ? ({ available: t("In the lounge"), waiting: t("At a table, waiting"), playing: t("Playing") }[d.presence.status] || d.presence.status) : null;
  return el("div", { class: "stack" },
    el("div", { class: "grid-2" }, RULES.map((r) => rulesStanding(u, r, d.presence && d.presence.rules === r))),
    el("div", { class: "kpis" },
      kpi({ label: t("Casual games"), value: fmtNum(u.casual_games), iconName: "users", tone: "success" }),
      kpi({ label: t("Potting"), value: pot, sub: c.best_run ? tf("best run {n}", { n: c.best_run }) : null, iconName: "target", tone: "warning" }),
      kpi({ label: t("Games left unfinished"), value: fmtNum(u.abandons), iconName: "kick", tone: u.abandons > 3 ? "danger" : "" }),
      kpi({ label: t("Name changes"), value: tf("{n} of {max}", { n: u.name_changes, max: 3 }), iconName: "edit" })),
    el("div", { class: "grid-2" },
      card(cardHead(t("Account")), el("div", { class: "card-body" }, el("dl", { class: "kv" },
        el("dt", {}, t("Username")), el("dd", {}, el("span", { class: "row" }, el("b", {}, u.username), btn(t("Copy"), { kind: "sm ghost icon-only", icon: "copy", iconOnly: true, onClick: () => copyText(u.username) }))),
        el("dt", {}, t("Role")), el("dd", {}, roleLabel(u.role)),
        el("dt", {}, t("Country")), el("dd", {}, country(u.country)),
        el("dt", {}, t("Recovery e-mail")), el("dd", {}, u.recovery_email || (d.private_hidden ? adminOnly() : el("span", { class: "faint" }, t("none")))),
        el("dt", {}, t("Payment e-mail")), el("dd", {}, d.payment_email || (d.private_hidden ? adminOnly() : el("span", { class: "faint" }, t("none")))),
        el("dt", {}, t("Created")), el("dd", {}, fmtDate(u.created_at)),
        el("dt", {}, t("Last seen")), el("dd", {}, u.online ? t("online now") : fmtDate(u.last_seen)),
        presence ? el("dt", {}, t("Right now")) : null, presence ? el("dd", { class: "row" }, presence, rulesBadge(d.presence.rules, true)) : null,
        el("dt", {}, t("Status message")), el("dd", {}, u.status_message ? el("span", {}, el("i", {}, "“" + u.status_message + "” "), btn(t("Clear"), { kind: "sm", onClick: () => playerAction(u, "clear-status", {}, t("Status message cleared.")) })) : el("span", { class: "faint" }, t("none")))))),
      card(cardHead(t("Standing")), el("div", { class: "card-body stack" },
        el("div", { class: "health" },
          healthRow(!(u.banned_until > nowSecs()), u.banned_until > nowSecs() ? (u.banned_until >= FOREVER ? t("Banned for good") : tf("Banned until {when}", { when: fmtDate(u.banned_until) })) : t("Not banned")),
          healthRow(!(u.muted_until > nowSecs()), u.muted_until > nowSecs() ? tf("Muted until {when}", { when: fmtDate(u.muted_until) }) : t("Not muted"), null, true),
          healthRow(d.reports.filter((r) => r.status === 0).length === 0, tn(d.reports.filter((r) => r.status === 0).length, "{n} open report about them", "{n} open reports about them"), tn(d.reports.length, "{n} in all", "{n} in all"), true),
          healthRow(!d.notes.some((n) => n.kind === "warning"), tn(d.notes.filter((n) => n.kind === "warning").length, "{n} warning", "{n} warnings"), null, true),
          healthRow(u.abandons <= 3, tn(u.abandons, "{n} game left unfinished", "{n} games left unfinished"), null, true)),
        el("div", { class: "row" }, btn(t("Open the moderation history"), { kind: "sm", icon: "shield", onClick: () => go("players/" + encodeURIComponent(u.username) + "/moderation") }))))));
}

const AUDIT_TONE = {
  ban: ["danger", "ban"], unban: ["success", "undo"], mute: ["warning", "mute"], unmute: ["success", "undo"], kick: ["warning", "kick"],
  warn: ["warning", "warn"], note: ["info", "note"], "note-remove": ["", "trash"], "logout-all": ["", "logout"], role: ["info", "shield"],
  "password-reset": ["warning", "key"], coins: ["success", "coins"], item: ["success", "gift"], vip: ["success", "star"], revoke: ["danger", "gift"],
  rename: ["info", "edit"], "clear-status": ["", "trash"], report: ["info", "flag"], announce: ["info", "megaphone"], "clear-chat": ["danger", "chat"],
  "chat-delete": ["warning", "chat"], motd: ["info", "megaphone"], pin: ["info", "pin"], unpin: ["", "pin"], "remove-seek": ["", "trash"],
  payment: ["", "coins"], "msstore-retry": ["", "refresh"], login: ["", "lock"], "tournament-cancel": ["danger", "trophy"],
  event: ["success", "calendar"], "event-cancel": ["danger", "calendar"],
};
const AUDIT_ACTIONS = {
  ban: "Banned", unban: "Ban lifted", mute: "Muted", unmute: "Unmuted", kick: "Disconnected", warn: "Warned", note: "Staff note",
  "note-remove": "Note removed", "logout-all": "Signed out everywhere", role: "Role changed", "password-reset": "Password set",
  coins: "Coins", item: "Item given", vip: "VIP given", revoke: "Item taken back", rename: "Renamed", "clear-status": "Status cleared",
  report: "Report answered", announce: "Announcement", "clear-chat": "Chat cleared", "chat-delete": "Chat line deleted", motd: "Welcome message",
  pin: "Note pinned", unpin: "Note taken down", "remove-seek": "Board note removed", payment: "Payment marked", "msstore-retry": "Store purchase sent again",
  login: "Signed in to the portal", "tournament-cancel": "Tournament called off",
  event: "Event scheduled", "event-cancel": "Event called off",
};
const auditLabel = (a) => t(AUDIT_ACTIONS[a] || a);
/** The actions whose target is a player (the others name an event, a
 * report, a payment, a place…, shown as plain text). */
const PLAYER_ACTIONS = new Set(["ban", "unban", "mute", "unmute", "kick", "warn", "note", "logout-all", "role", "password-reset", "coins", "item",
  "vip", "revoke", "rename", "clear-status", "remove-seek", "chat-delete"]);
const AUDIT_PLACES = { lounge: "the lounge", board: "the board" };
const auditTarget = (e) => (AUDIT_PLACES[e.target] ? t(AUDIT_PLACES[e.target]) : e.target);
function auditMini(e) {
  const [tone, ic] = AUDIT_TONE[e.action] || ["", "info"];
  return el("div", { class: "mi" }, el("span", { class: "tl-dot " + tone }, icon(ic)),
    el("div", { class: "mi-main" }, el("span", {}, el("b", {}, e.staff), " · ", auditLabel(e.action), e.target && e.action !== "login" ? " · " : null, e.action !== "login" ? auditTarget(e) : null),
      e.details && e.action !== "login" ? el("span", { class: "mi-sub ellipsis" }, e.details) : null),
    el("span", { class: "mi-side" }, timeEl(e.at)));
}

function playerModeration(d) {
  const u = d.user;
  const notes = d.notes.length ? el("div", { class: "stack" }, d.notes.map((n) => {
    const mine = n.author === me.username;
    return el("div", { class: "note-card " + (n.kind === "warning" ? "warning" : "") },
      el("div", { class: "note-head" },
        el("span", { class: "row" }, badge(n.kind === "warning" ? t("Warning") : t("Note"), n.kind === "warning" ? "warning" : "info", n.kind === "warning" ? "warn" : "note"),
          el("span", {}, tf("{name}, {when}", { name: n.author, when: ago(n.created_at) }))),
        el("span", { class: "row" },
          n.kind === "warning" ? (n.delivered_at ? badge(tf("seen {when}", { when: ago(n.delivered_at) }), "success", "check") : badge(t("waiting for the next sign-in"), "", "clock")) : null,
          mine || isAdmin() ? btn(t("Remove"), { kind: "sm ghost icon-only", icon: "trash", iconOnly: true, onClick: async () => {
            if (await confirmDialog({ title: t("Remove this note?"), text: t("It is gone for every staff member. The audit log keeps that it was removed."), confirm: t("Remove") })) {
              if (await attempt(() => api("/notes/" + n.id, { method: "DELETE" }), t("Note removed."))) refreshIf("players");
            }
          } }) : null)),
      el("div", { class: "note-text" }, n.text));
  })) : empty("note", t("No notes yet"), t("Notes stay among the staff; warnings are shown to the player."));
  const history = d.moderation.length ? el("div", { class: "timeline" }, d.moderation.map((e) => {
    const [tone, ic] = AUDIT_TONE[e.action] || ["", "info"];
    return el("div", { class: "tl-item" }, el("div", { class: "tl-dot " + tone }, icon(ic)),
      el("div", { class: "tl-body" }, el("span", { class: "tl-title" }, auditLabel(e.action)),
        e.details ? el("span", { class: "tl-text" }, e.action === "login" ? tf("from {ip}", { ip: e.details }) : e.details) : null,
        el("span", { class: "tl-meta" }, tf("by {name}", { name: e.staff }), " · ", timeEl(e.at))));
  })) : empty("shield", t("A clean record"), t("No staff action has been taken on this account."));
  const reports = el("div", { class: "stack card-body" });
  const drawReports = (all) => {
    const shown = all ? d.reports : d.reports.slice(0, 5);
    setKids(reports, shown.map((r) => reportCard(r, { compact: true })),
      d.reports.length > shown.length ? el("div", { class: "row" }, btn(tf("Show all {n}", { n: fmtNum(d.reports.length) }), { kind: "sm", onClick: () => drawReports(true) })) : null);
  };
  drawReports(false);
  return el("div", { class: "grid-aside" },
    el("div", { class: "stack" },
      card(cardHead(t("Staff notes and warnings"), null, btn(t("Note"), { kind: "sm", icon: "note", onClick: () => noteDialog(u, "note") }), u.deleted_at ? null : btn(t("Warn"), { kind: "sm", icon: "warn", onClick: () => noteDialog(u, "warning") })),
        el("div", { class: "card-body" }, notes)),
      card(cardHead(tf("Reports about {name}", { name: u.username }), tn(d.reports.length, "{n} report", "{n} reports")),
        d.reports.length ? reports : empty("flag", t("No reports"), t("Nobody has reported this player.")))),
    card(cardHead(t("History"), t("Every staff action on this account")), el("div", { class: "card-body" }, history)));
}

function playerGames(d) {
  if (!d.history.length) return card(null, empty("trophy", t("No games yet"), t("Their online games show up here.")));
  const count = (r) => d.history.filter((h) => h.rules === r).length;
  let only = "all";
  const holder = el("div");
  const filter = el("div");
  const draw = () => {
    setKids(filter, segmented([["all", t("All"), d.history.length], ...RULES.map((r) => [r, rulesShort(r), count(r)])], only, (v) => { only = v; draw(); }, t("Rules")));
    const list = d.history.filter((h) => only === "all" || h.rules === only);
    setKids(holder, list.length ? table([t("When"), t("Rules"), t("Opponent"), t("Result"), t("How it ended"), { label: t("Rating"), cls: "num" }, { label: t("After"), cls: "num" }, ""], list.map(gameRow))
      : empty("trophy", t("No games under these rules"), null));
  };
  draw();
  return card(cardHead(t("Latest games"), tn(d.history.length, "{n} game", "{n} games"), filter), holder);
}
function gameRow(h) {
  return el("tr", {},
    el("td", {}, fmtDate(h.at)),
    el("td", {}, rulesBadge(h.rules)),
    el("td", {}, personLink(h.opponent)),
    el("td", {}, h.won ? badge(t("Won"), "success") : badge(t("Lost"), "danger")),
    el("td", {}, gameEnd(h.reason)),
    el("td", { class: "num" }, h.rated ? el("b", { class: "delta" + (h.delta > 0 ? " up" : h.delta < 0 ? " down" : "") }, (h.delta > 0 ? "+" : "") + fmtNum(h.delta)) : badge(t("Casual"))),
    el("td", { class: "num" }, h.rated ? fmtNum(h.rating_after) : "—"),
    el("td", { class: "actions-cell" }, h.replay ? el("span", { class: "row" },
      btn(t("Watch"), { kind: "sm", icon: "play", onClick: () => openViewer("/replay/" + h.match_id + "/view") }),
      btn(t("Download"), { kind: "sm ghost icon-only", icon: "download", iconOnly: true, onClick: () => downloadReplay(h.match_id) })) : el("span", { class: "faint" }, t("no replay"))));
}
const GAME_ENDS = {
  pocketed_eight: "8-ball made", eight_pocketed_early: "8-ball made too early", eight_in_wrong_pocket: "8-ball in the wrong pocket",
  eight_pocketed_on_foul: "8-ball made on a foul", eight_off_table: "8-ball off the table", eight_with_another_ball: "8-ball with another ball",
  resignation: "Resigned", abandoned: "Left the game",
};
const gameEnd = (r) => t(GAME_ENDS[r] || String(r).replaceAll("_", " "));

function playerEconomy(d) {
  const u = d.user, w = d.wallet;
  const vip = w.vip_until > nowSecs();
  const owned = w.owned || [];
  const items = owned.length ? el("div", { class: "item-chips" }, owned.map((code) => el("span", { class: "item-chip" + (isAdmin() ? "" : " plain") },
    el("span", { class: "mono" }, code),
    isAdmin() ? btn(t("Take back"), { kind: "sm ghost icon-only", icon: "x", iconOnly: true, onClick: () => revokeDialog(u, code) }) : null)))
    : el("p", { class: "faint" }, t("No items."));
  const payments = d.private_hidden ? empty("lock", t("Visible to administrators only"), null) : d.payments.length ? table([t("When"), t("What"), { label: t("Coins"), cls: "num" }, { label: t("Amount"), cls: "num" }, t("Status"), t("Payment"), ""], d.payments.map((p) => el("tr", {},
    el("td", {}, fmtDate(p.at)), el("td", {}, p.kind === "vip" ? "VIP" : t("Coins")), el("td", { class: "num" }, fmtNum(p.coins)),
    el("td", { class: "num" }, money(p.amount, p.currency)),
    el("td", {}, el("span", { class: "badges" }, p.refunded ? badge(t("Refunded"), "danger") : badge(t("Paid"), "success"), p.test ? badge(t("Test"), "warning") : null)),
    el("td", { class: "mono faint" }, p.id),
    el("td", { class: "actions-cell" }, isAdmin() ? btn(p.test ? t("Mark as real") : t("Mark as test"), { kind: "sm ghost", onClick: () => markPayment(p, () => refreshIf("players")) }) : null))))
    : empty("coins", t("No payments"), null);
  const ms = (d.msstore || []).length ? card(cardHead(t("Microsoft Store purchases")), table([t("Found"), t("Pack"), { label: t("Coins"), cls: "num" }, t("State"), { label: t("Tries"), cls: "num" }, t("Last error"), t("Orders"), ""], d.msstore.map((r) => el("tr", {},
    el("td", {}, fmtDate(r.created_at)), el("td", {}, r.pack), el("td", { class: "num" }, fmtNum(r.coins)),
    el("td", {}, badge({ done: t("Done"), pending: t("Pending"), stuck: t("Stuck"), failed: t("Failed") }[r.state] || r.state, r.state === "done" ? "success" : r.state === "pending" ? "warning" : "danger")),
    el("td", { class: "num" }, fmtNum(r.attempts)), el("td", { class: "wrap faint" }, r.last_error || ""), el("td", { class: "mono faint" }, r.order_ids || ""),
    el("td", { class: "actions-cell" }, r.state === "stuck" && isAdmin() ? btn(t("Send again"), { kind: "sm", icon: "refresh", onClick: async () => {
      if (await attempt(() => api("/msstore/" + encodeURIComponent(r.tracking_id) + "/retry", { method: "POST" }), t("Sent to Microsoft again."))) refreshIf("players");
    } }) : null))))) : null;
  return el("div", { class: "stack" },
    el("div", { class: "kpis" },
      kpi({ label: t("Coins"), value: fmtNum(w.coins), iconName: "coins", tone: "warning" }),
      kpi({ label: "VIP", value: vip ? t("Yes") : t("No"), sub: vip ? tf("until {when}", { when: fmtDateOnly(w.vip_until) }) : w.vip_until ? tf("ended {when}", { when: ago(w.vip_until) }) : null, iconName: "star", tone: vip ? "success" : "" }),
      kpi({ label: t("Items"), value: fmtNum(owned.length), iconName: "gift", tone: "info" }),
      kpi({ label: t("Payments"), value: d.private_hidden ? "—" : fmtNum(d.payments.length), sub: d.private_hidden ? t("Visible to administrators only") : null, iconName: "coins" })),
    card(cardHead(t("Items owned"), null,
      isAdmin() ? btn(t("Coins"), { kind: "sm", icon: "coins", onClick: () => coinsDialog(u) }) : null,
      isAdmin() ? btn(t("Item"), { kind: "sm", icon: "gift", onClick: () => itemDialog(u) }) : null,
      isAdmin() ? btn("VIP", { kind: "sm", icon: "star", onClick: () => vipDialog(u) }) : null), el("div", { class: "card-body" }, items)),
    card(cardHead(t("Payments")), payments),
    ms,
    card(cardHead(t("Coin history"), t("The latest 50 changes")), d.ledger.length ? table([t("When"), { label: t("Coins"), cls: "num" }, t("Why")], d.ledger.map((l) => el("tr", {},
      el("td", {}, fmtDate(l.at)), el("td", { class: "num" }, el("b", {}, (l.delta > 0 ? "+" : "") + fmtNum(l.delta))), el("td", { class: "wrap" }, l.reason)))) : empty("coins", t("No coins moved yet"), null)));
}

/** What the player did against the computer: the tour, the medals of its
 * tournaments, the drills and the daily challenges (as the server knows
 * them: progress on the tour before its final stays in the player's game). */
function playerComputer(d) {
  const c = d.computer || {};
  const medals = c.medals || { knockout: [0, 0, 0], arena: [0, 0, 0] };
  const sum = (a) => a[0] + a[1] + a[2];
  const allMedals = sum(medals.knockout) + sum(medals.arena);
  const golds = medals.knockout[0] + medals.arena[0];
  const formatName = (f) => (f === "arena" ? t("Arena") : t("Knockout"));
  const placeName = (p) => [t("Gold"), t("Silver"), t("Bronze")][p - 1] || String(p);
  const placeTone = (p) => ["warning", "", "danger"][p - 1] || "";
  const finals = c.finals || 0;
  const tourSub = c.tour_won_at ? tf("on {when}", { when: fmtDate(c.tour_won_at) })
    : finals ? tn(finals, "{n} final played", "{n} finals played") : t("No final played yet");
  const medalRow = (label, m) => el("tr", {},
    el("td", {}, label), el("td", { class: "num" }, fmtNum(m[0])), el("td", { class: "num" }, fmtNum(m[1])), el("td", { class: "num" }, fmtNum(m[2])),
    el("td", { class: "num" }, fmtNum(sum(m))));
  const recent = c.recent_medals || [];
  const drills = (c.drills || []).slice().sort((a, b) => b.stars - a.stars || b.best - a.best);
  return el("div", { class: "stack" },
    el("div", { class: "kpis cols-3" },
      kpi({ label: t("The tour"), value: c.tour_won_at ? t("Won") : t("Not won yet"), sub: tourSub, iconName: "crown", tone: c.tour_won_at ? "success" : "" }),
      kpi({ label: t("Tournament medals"), value: fmtNum(allMedals), sub: tn(golds, "{n} gold", "{n} gold"), iconName: "trophy", tone: "warning" }),
      kpi({ label: t("Drill stars"), value: tf("{n} of {max}", { n: c.drill_stars || 0, max: c.drill_stars_max || 0 }), sub: tn(drills.length, "{n} drill done", "{n} drills done"), iconName: "target", tone: "info" }),
      kpi({ label: t("Daily challenges topped"), value: fmtNum(c.daily_wins || 0), iconName: "calendar" })),
    card(cardHead(t("The tour"), t("Its final is checked by the server; the stages before it stay in the player's game.")),
      el("div", { class: "card-body stack" },
        el("div", {}, c.tour_won_at ? tf("Won the final on {when}: the tour's cue and achievement.", { when: fmtDate(c.tour_won_at) }) : t("Has not won the tour's final yet.")),
        el("div", { class: "faint" }, finals
          ? tf("{n} finals dealt by the server (of the latest {kept} kept), the last on {when}.", { n: fmtNum(finals), kept: c.finals_kept || finals, when: fmtDate(c.last_final) })
          : t("No final dealt yet.")))),
    card(cardHead(t("Medals of the tournaments against the computer"), t("Kept in the account once the player is online; shown only here and on the game's Tournament tab.")),
      table([t("Format"), { label: t("Gold"), cls: "num" }, { label: t("Silver"), cls: "num" }, { label: t("Bronze"), cls: "num" }, { label: t("Total"), cls: "num" }],
        [medalRow(t("Knockout"), medals.knockout), medalRow(t("Arena"), medals.arena)])),
    recent.length ? card(cardHead(t("Latest medals"), tn(recent.length, "{n} shown", "{n} shown")),
      table([t("When"), t("Medal"), t("Format"), t("Tournament")], recent.map((m) => el("tr", {},
        el("td", {}, fmtDate(m.at)),
        el("td", {}, el("span", { class: ("badge " + placeTone(m.place)).trim() }, placeName(m.place))),
        el("td", {}, formatName(m.format)),
        el("td", { class: "mono faint" }, "#" + m.cup))))) : null,
    card(cardHead(t("Drills"), tf("{n} of {max} stars", { n: c.drill_stars || 0, max: c.drill_stars_max || 0 })),
      drills.length ? table([t("Drill"), { label: t("Best"), cls: "num" }, t("Stars")], drills.map((x) => el("tr", {},
        el("td", {}, x.name ? t(x.name) : x.code),
        el("td", { class: "num" }, fmtNum(x.best)),
        el("td", {}, "★".repeat(x.stars) + "☆".repeat(Math.max(0, 3 - x.stars))))))
        : el("div", { class: "card-body" }, empty("target", t("No drill done yet"), null))));
}

function playerAchievements(d) {
  if (!d.achievements.length) return card(null, empty("star", t("No achievements yet"), null));
  return card(cardHead(t("Achievements"), tn(d.achievements.length, "{n} unlocked", "{n} unlocked")), el("div", { class: "card-body item-chips" },
    d.achievements.map(([code, at]) => el("span", { class: "item-chip plain", title: fmtDate(at) }, icon("star"), el("span", { class: "mono" }, code), el("span", { class: "faint" }, fmtDateOnly(at))))));
}

// ------------------------------------------------------ actions on a player

async function playerAction(u, action, body, done) {
  const ok = await attempt(() => api("/users/" + encodeURIComponent(u.username) + "/" + action, { method: "POST", body }), done);
  if (ok) refreshIf("players");
  return ok;
}
/** The same, for a form (throws, so the dialog shows the error). */
async function postAction(name, action, body) {
  return api("/users/" + encodeURIComponent(name) + "/" + action, { method: "POST", body });
}
const REASONS = [["Cheating", "Cheating"], ["Harassment", "Harassment or insults"], ["Offensive name", "Offensive name or status"], ["Spam", "Spam or flooding"], ["Leaving games", "Leaving games on purpose"]];
function reasonField(label, required) {
  return {
    name: "reason", label, type: "textarea", rows: 2, max: 300, required, placeholder: t("Write the reason, or pick one above"),
  };
}
function reasonChips(getInput) {
  return el("div", { class: "chips" }, REASONS.map(([, l]) => el("button", { type: "button", class: "chip", onclick: () => { const i = getInput(); i.value = t(l); i.dispatchEvent(new Event("input")); i.focus(); } }, t(l))));
}
function noteDialog(u, kind, after) {
  const warning = kind === "warning";
  let input;
  const chips = warning ? reasonChips(() => input) : null;
  const d = formDialog({
    title: warning ? tf("Warn {name}", { name: u.username }) : tf("A staff note about {name}", { name: u.username }),
    text: warning ? (u.online ? t("They see it at once, on their screen.") : t("They see it when they next sign in.")) : t("Only the staff see notes. Keep them factual."),
    iconName: warning ? "warn" : "note", tone: warning ? "warning" : "info",
    fields: [
      ...(warning ? [{ type: "node", node: chips }] : []),
      { name: "text", label: warning ? t("Warning") : t("Note"), type: "textarea", rows: 4, max: 600, required: true, placeholder: warning ? t("What they did, and what happens next time") : t("What happened, links, what to watch for") },
    ],
    extra: warning ? el("div", { class: "preview-box" }, el("span", { class: "faint" }, t("The player reads (in their language)")), t("Warning from the staff:") + " …") : null,
    submit: warning ? t("Send the warning") : t("Save the note"), submitKind: warning ? "primary" : "primary",
    onSubmit: async (v) => {
      await postAction(u.username, warning ? "warn" : "note", { reason: oneLine(v.text) });
      toast(warning ? t("Warning sent.") : t("Note saved."), "success");
      if (after) after(); else refreshIf("players");
    },
  });
  input = d.root.querySelector("textarea");
  return d;
}
function muteDialog(u, after) {
  let input;
  const d = formDialog({
    title: tf("Mute {name}", { name: u.username }), text: t("They can play but not write in the chat, nor post on the board."), iconName: "mute", tone: "warning",
    fields: [
      { name: "minutes", label: t("For how long"), type: "choices", value: "60", options: [["15", fmtMinutes(15)], ["60", fmtMinutes(60)], ["1440", fmtMinutes(1440)], ["10080", fmtMinutes(10080)], ["43200", fmtMinutes(43200)]] },
      { type: "node", node: reasonChips(() => input) },
      reasonField(t("Reason (shown to the player)"), true),
    ],
    submit: t("Mute"), submitKind: "danger",
    onSubmit: async (v) => {
      await postAction(u.username, "mute", { minutes: Number(v.minutes), reason: oneLine(v.reason) });
      toast(tf("{name} is muted for {time}.", { name: u.username, time: fmtMinutes(Number(v.minutes)) }), "success");
      if (after) after(fmtMinutes(Number(v.minutes))); else refreshIf("players");
    },
  });
  input = d.root.querySelector("textarea");
}
function banDialog(u, after) {
  let input;
  const preview = el("div", { class: "preview-box" });
  const d = formDialog({
    title: tf("Ban {name}", { name: u.username }), text: t("They are signed out at once and can't sign in until the ban ends."), iconName: "ban", tone: "danger",
    fields: [
      { name: "hours", label: t("For how long"), type: "choices", value: "72", options: [["24", tn(1, "{n} day", "{n} days")], ["72", tn(3, "{n} day", "{n} days")], ["168", tn(1, "{n} week", "{n} weeks")], ["720", tn(30, "{n} day", "{n} days")], ["0", t("For good")]],
        onInput: () => updatePreview() },
      { type: "node", node: reasonChips(() => input) },
      reasonField(t("Reason (shown to the player)"), true),
    ],
    extra: preview,
    submit: t("Ban"), submitKind: "danger",
    onSubmit: async (v) => {
      const hours = Number(v.hours);
      await postAction(u.username, "ban", { hours: hours || null, reason: oneLine(v.reason) });
      toast(tf("{name} is banned.", { name: u.username }), "success");
      if (after) after(hours ? fmtMinutes(hours * 60) : t("for good")); else refreshIf("players");
    },
  });
  input = d.root.querySelector("textarea");
  const choices = d.root.querySelector(".chips[role=radiogroup]");
  function updatePreview() {
    const h = Number(choices.value);
    setKids(preview, el("span", { class: "faint" }, t("The player reads")),
      h ? tf("Your account was suspended for {hours} hours: {reason}", { hours: h, reason: oneLine(input.value) || "…" }) : tf("Your account was banned: {reason}", { reason: oneLine(input.value) || "…" }));
  }
  input.addEventListener("input", updatePreview);
  updatePreview();
}
async function unbanDialog(u) {
  formDialog({
    title: tf("Lift the ban on {name}?", { name: u.username }), text: t("They can sign in again at once."), iconName: "undo", tone: "info",
    fields: [{ name: "reason", label: t("Why"), type: "text", optional: true, max: 300 }],
    submit: t("Lift the ban"),
    onSubmit: async (v) => { await postAction(u.username, "unban", { reason: v.reason }); toast(t("Ban lifted."), "success"); refreshIf("players"); },
  });
}
function kickDialog(u) {
  formDialog({
    title: tf("Disconnect {name}?", { name: u.username }), text: t("Their connection ends now; they can come back at once. A game in progress waits for them."), iconName: "kick", tone: "warning",
    fields: [{ name: "reason", label: t("Reason (shown to the player)"), type: "text", optional: true, max: 300 }],
    submit: t("Disconnect"), submitKind: "danger",
    onSubmit: async (v) => { await postAction(u.username, "kick", { reason: oneLine(v.reason) }); toast(t("Disconnected."), "success"); refreshIf("players"); },
  });
}
function renameDialog(u) {
  formDialog({
    title: tf("Rename {name}", { name: u.username }), text: t("For a name that breaks the rules. It doesn't use one of the player's own changes; they are signed out and sign in again under the new name."), iconName: "edit",
    fields: [
      { name: "name", label: t("New username"), type: "text", required: true, value: "", hint: t("3 to 16 letters, digits or underscores."),
        validate: (v) => (/^[A-Za-z0-9_]{3,16}$/.test(v.trim()) ? null : t("Usernames have 3–16 letters, digits or underscores.")) },
      { name: "reason", label: t("Why"), type: "text", optional: true, max: 300 },
    ],
    submit: t("Rename"),
    onSubmit: async (v) => {
      const r = await postAction(u.username, "rename", { name: v.name.trim(), reason: v.reason });
      toast(tf("Renamed to {name}.", { name: r.user.username }), "success");
      go("players/" + encodeURIComponent(r.user.username));
    },
  });
}
function roleDialog(u) {
  formDialog({
    title: tf("The role of {name}", { name: u.username }), text: t("They are signed out so the new role takes effect."), iconName: "shield",
    fields: [
      { name: "role", label: t("Role"), type: "choices", value: u.role, options: [["player", roleLabel("player")], ["moderator", roleLabel("moderator")], ["admin", roleLabel("admin")]] },
      { type: "node", node: el("div", { class: "hint stack" },
        el("span", {}, el("b", {}, roleLabel("moderator") + ": "), t("reports, warnings, mutes, bans, the chat and the board.")),
        el("span", {}, el("b", {}, roleLabel("admin") + ": "), t("everything, with roles, renames, passwords, coins and the server."))) },
    ],
    submit: t("Save"),
    onSubmit: async (v) => { await postAction(u.username, "role", { role: v.role }); toast(t("Role changed."), "success"); refreshIf("players"); },
  });
}
function passwordDialog(u) {
  formDialog({
    title: tf("A new password for {name}", { name: u.username }), text: t("Only after confirming who they are (their recovery e-mail). Every session of the account ends."), iconName: "key", tone: "warning",
    fields: [
      { name: "password", label: t("New password"), type: "password", required: true, autocomplete: "new-password", hint: t("At least 8 characters, not only digits."),
        validate: (v) => (v.length < 8 || /^\d+$/.test(v) ? t("Passwords need at least 8 characters (not only digits).") : null) },
      { name: "again", label: t("The same again"), type: "password", required: true, autocomplete: "new-password",
        validate: (v, all) => (v !== all.password ? t("The passwords are not the same.") : null) },
    ],
    submit: t("Set the password"), submitKind: "danger",
    onSubmit: async (v) => { await postAction(u.username, "password", { password: v.password }); toast(t("Password set. Tell the player through their recovery e-mail."), "success"); refreshIf("players"); },
  });
}
function coinsDialog(u) {
  formDialog({
    title: tf("Coins for {name}", { name: u.username }), text: t("A negative amount takes coins away. Always with a reason: it goes to the audit log."), iconName: "coins",
    fields: [
      { name: "coins", label: t("Coins"), type: "number", required: true, step: 1, placeholder: "500",
        validate: (v) => (!Number.isInteger(Number(v)) || Number(v) === 0 || Math.abs(Number(v)) > 1000000 ? t("A whole number, not zero, up to a million.") : null) },
      { name: "reason", label: t("Reason"), type: "text", required: true, max: 300 },
    ],
    submit: t("Apply"),
    onSubmit: async (v) => { await postAction(u.username, "coins", { coins: Number(v.coins), reason: v.reason }); toast(t("Coins changed."), "success"); refreshIf("players"); },
  });
}
let catalogCache = null;
async function catalog() {
  if (!catalogCache) catalogCache = (await api("/economy")).catalog;
  return catalogCache;
}
async function itemDialog(u) {
  let items;
  try { items = await catalog(); } catch (e) { toast(e.message, "error"); return; }
  formDialog({
    title: tf("An item for {name}", { name: u.username }), iconName: "gift",
    fields: [
      { name: "item", label: t("Item"), type: "select", options: items.map((i) => [i.code, i.name + " · " + t(CATEGORY[i.category] || i.category) + " · " + i.code]) },
      { name: "reason", label: t("Reason"), type: "text", required: true, max: 300 },
    ],
    submit: t("Give"),
    onSubmit: async (v) => { await postAction(u.username, "item", { item: v.item, reason: v.reason }); toast(t("Item given."), "success"); refreshIf("players"); },
  });
}
function vipDialog(u) {
  formDialog({
    title: tf("VIP for {name}", { name: u.username }), text: t("Added to the VIP they have (or from today)."), iconName: "star",
    fields: [
      { name: "days", label: t("Days"), type: "choices", value: "30", options: [["7", tn(7, "{n} day", "{n} days")], ["30", tn(30, "{n} day", "{n} days")], ["90", tn(90, "{n} day", "{n} days")], ["365", tn(365, "{n} day", "{n} days")]] },
      { name: "reason", label: t("Reason"), type: "text", required: true, max: 300 },
    ],
    submit: t("Give"),
    onSubmit: async (v) => { await postAction(u.username, "vip", { days: Number(v.days), reason: v.reason }); toast(t("VIP given."), "success"); refreshIf("players"); },
  });
}
function revokeDialog(u, code) {
  formDialog({
    title: tf("Take {item} back from {name}?", { item: code, name: u.username }), text: t("For an item given by mistake, or a refunded purchase. They stop using it at once."), iconName: "gift", tone: "danger",
    fields: [{ name: "reason", label: t("Reason"), type: "text", required: true, max: 300 }],
    submit: t("Take back"), submitKind: "danger",
    onSubmit: async (v) => { await postAction(u.username, "revoke", { item: code, reason: v.reason }); toast(t("Item taken back."), "success"); refreshIf("players"); },
  });
}
async function markPayment(p, after) {
  const ok = await confirmDialog({
    title: p.test ? t("Count this payment as real money?") : t("Mark this payment as a test?"),
    text: t("Test payments (the provider's test keys) stay out of the revenue."), confirm: p.test ? t("Mark as real") : t("Mark as test"), tone: "warning", iconName: "coins",
  });
  if (ok && await attempt(() => api("/payments/" + encodeURIComponent(p.id) + "/test", { method: "POST", body: { test: !p.test } }), t("Saved."))) after();
}

// =================================================================== reports

// Reports keep their reason as the game names it (in English).
const REPORT_REASONS = { "Harassment": "Harassment", "Cheating or abuse": "Cheating or abuse", "Offensive name": "Offensive name", "Spam": "Spam", "Other": "Other" };
function reasonBadge(r) {
  const tone = { "Cheating or abuse": "danger", "Harassment": "warning", "Offensive name": "info", "Spam": "accent" }[r] || "";
  return badge(t(REPORT_REASONS[r] || r), tone);
}

PAGES.reports = async (route) => {
  const status = route.query.get("status") || "open";
  const reason = route.query.get("reason") || "";
  const who = route.query.get("q") || "";
  const d = await api("/reports?" + new URLSearchParams({ status }));
  if (status === "open") { counts.reports = d.reports.length; renderSidebar(); }
  const about = {};
  for (const r of d.reports) about[r.target] = (about[r.target] || 0) + 1;
  const shown = d.reports.filter((r) => (!reason || r.reason === reason)
    && (!who || r.target.toLowerCase().includes(who.toLowerCase()) || r.reporter.toLowerCase().includes(who.toLowerCase())));
  const search = el("input", { class: "input wide", type: "search", placeholder: t("Player (reported or reporting)"), value: who, "aria-label": t("Search"), "data-focus": "reports-q" });
  search.addEventListener("input", debounce(() => { setQuery({ q: search.value.trim() }); render({ quiet: true }); }, 300));
  const reasonSel = el("select", { class: "select", "aria-label": t("Reason"), "data-focus": "reports-reason" }, el("option", { value: "" }, t("Every reason")),
    Object.entries(REPORT_REASONS).map(([v, l]) => el("option", { value: v }, t(l))));
  reasonSel.value = reason;
  reasonSel.addEventListener("change", () => { setQuery({ reason: reasonSel.value }); render(); });
  return el("div", {},
    pageHead(t("Reports"), status === "open" ? tn(d.reports.length, "{n} report waiting for review", "{n} reports waiting for review") : tn(d.reports.length, "{n} report", "{n} reports"), updatedLabel(refreshPage)),
    el("div", { class: "toolbar" },
      segmented([["open", t("Open")], ["actioned", t("Action taken")], ["dismissed", t("Dismissed")], ["all", t("All")]], status, (v) => { setQuery({ status: v }); render(); }, t("Status")),
      reasonSel, search),
    shown.length ? el("div", { class: "stack" }, shown.map((r) => reportCard(r, { about: about[r.target] })))
      : card(null, status === "open" ? empty("checkCircle", t("All caught up"), t("No report waits for review.")) : empty("flag", t("No reports here"), null)));
};

function reportCard(r, { compact = false, about = 0 } = {}) {
  const open = r.status === 0;
  // data-keep: a note being written survives the page refreshing after
  // another report is answered.
  const note = el("input", { class: "input", placeholder: t("A note on what was done (optional)"), maxlength: "300", "aria-label": t("Note"), "data-keep": "report-note-" + r.id });
  const target = { username: r.target, online: false };
  const answerBtns = [];
  let sending = false;
  const resolve = async (st, text) => {
    // One answer at a time: the buttons are off while it is sent.
    if (sending) return;
    sending = true;
    for (const b of answerBtns) b.disabled = true;
    try {
      const ok = await attempt(() => api("/reports/" + encodeURIComponent(r.id), { method: "POST", body: { status: st, note: oneLine(text ?? note.value) } }),
        st === "dismissed" ? t("Report dismissed.") : st === "open" ? t("Report reopened.") : t("Report closed."));
      if (ok) { refreshIf("reports"); refreshIf("players"); pollCounts(); }
    } finally {
      sending = false;
      for (const b of answerBtns) b.disabled = false;
    }
  };
  const sanction = (kind) => {
    const after = (what) => resolve("actioned", [what, note.value].filter(Boolean).join(" · "));
    if (kind === "warn") noteDialog(target, "warning", () => after(t("Warned")));
    if (kind === "mute") muteDialog(target, (time) => after(tf("Muted for {time}", { time })));
    if (kind === "ban") banDialog(target, (time) => after(tf("Banned ({time})", { time })));
  };
  const answer = (label, opts) => { const b = btn(label, opts); answerBtns.push(b); return b; };
  // About oneself: a moderator leaves it to an administrator; an
  // administrator closes it or dismisses it (no sanctions on oneself).
  const aboutMe = !!me && String(r.target || "").toLowerCase() === me.username.toLowerCase();
  const mayAnswer = !aboutMe || isAdmin();
  const resolvedLine = !open ? el("div", { class: "row muted" },
    badge(r.status === 1 ? t("Action taken") : t("Dismissed"), r.status === 1 ? "success" : ""),
    el("span", {}, tf("by {name}", { name: r.resolved_by || "?" })), r.resolution ? el("span", {}, "· " + r.resolution) : null,
    compact || !mayAnswer ? null : answer(t("Reopen"), { kind: "sm ghost", icon: "undo", onClick: () => resolve("open", "") })) : null;
  return el("article", { class: "card report-card" },
    el("div", { class: "report-top" }, reasonBadge(r.reason), el("span", { class: "faint mono" }, "#" + r.id),
      about > 1 ? badge(tn(about, "{n} report about them", "{n} reports about them"), "danger") : null,
      el("span", { class: "when" }, timeEl(r.created_at))),
    compact ? el("div", { class: "parties" }, t("By"), personLink(r.reporter)) : el("div", { class: "parties" }, personLink(r.target), el("span", { class: "arrow" }, t("reported by")), personLink(r.reporter)),
    r.details ? el("p", { class: "details" }, r.details) : null,
    gameBlock(r),
    r.context ? el("details", {}, el("summary", { class: "muted" }, t("The chat before the report")), el("pre", { class: "context" }, r.context)) : null,
    open && !compact && !mayAnswer ? el("div", { class: "row muted" }, t("An administrator answers reports about you.")) : null,
    open && !compact && mayAnswer ? el("div", { class: "resolve" }, note,
      aboutMe ? null : answer(t("Warn"), { kind: "sm", icon: "warn", onClick: () => sanction("warn") }),
      aboutMe ? null : answer(t("Mute"), { kind: "sm", icon: "mute", onClick: () => sanction("mute") }),
      aboutMe ? null : answer(t("Ban"), { kind: "sm danger-soft", icon: "ban", onClick: () => sanction("ban") }),
      answer(t("Dismiss"), { kind: "sm", onClick: () => resolve("dismissed") }),
      answer(t("Close: action taken"), { kind: "sm primary", icon: "check", onClick: () => resolve("actioned") })) : null,
    open && compact ? el("div", { class: "row" }, btn(t("Open in reports"), { kind: "sm ghost", onClick: () => go("reports") })) : null,
    resolvedLine);
}

/** The game that came with a report. */
function gameBlock(r) {
  const g = r.game;
  if (!g) return el("div", { class: "game-attach none" }, t("No game between them in the 24 hours before the report."));
  if (g.kind === "gone") return el("div", { class: "game-attach none" }, t("They were playing when it was sent, but that game was never stored (the server restarted)."));
  const live = g.kind === "live";
  const when = live ? (g.finished ? t("just ended") : t("in progress when reported")) : tf("ended {when}", { when: ago(g.at) });
  return el("div", { class: "game-attach" },
    el("div", { class: "gi" }, el("b", {}, (live ? t("Game in progress") : t("The game")) + ": " + g.players[0] + " × " + g.players[1]),
      el("span", { class: "row" }, rulesBadge(g.rules), [when, g.rated ? t("rated") : t("casual"), tn(g.shots, "{n} stroke", "{n} strokes")].join(" · "))),
    el("div", { class: "row" },
      btn(live ? t("Watch so far") : t("Watch"), { kind: "sm primary", icon: "play", onClick: () => openViewer(live ? "/live/" + g.id + "/view" : "/replay/" + g.id + "/view") }),
      live ? null : btn(t("Download"), { kind: "sm ghost icon-only", icon: "download", iconOnly: true, onClick: () => downloadReplay(g.id) })));
}

// ====================================================================== live

PAGES.live = async (route) => {
  const tab = route.query.get("tab") || "games";
  const [d, s] = await Promise.all([api("/live"), api("/system")]);
  every(10000, () => refreshIf("live"));
  const playing = d.rooms.filter((r) => !r.finished);
  const tabs = el("div", { class: "tabs" }, [["games", t("Games"), playing.length], ["players", t("Players online"), d.players.length], ["tournaments", t("Tournaments"), d.tournaments.length]]
    .map(([id, label, n]) => el("button", { type: "button", class: "tab" + (tab === id ? " on" : ""), onclick: () => { setQuery({ tab: id }); render(); } }, label, el("span", { class: "count" }, fmtNum(n)))));
  let body;
  if (tab === "players") {
    // The filter lives in the address, so the refresh every 10 seconds keeps it.
    const filter = el("input", { class: "input wide", type: "search", placeholder: t("Filter by name"), "aria-label": t("Filter"), value: route.query.get("who") || "", "data-focus": "live-who" });
    const rows = () => {
      const s = filter.value.trim().toLowerCase();
      return d.players.filter((p) => !s || p.username.toLowerCase().includes(s)).map((p) => clickable(el("tr", { class: "click" },
        el("td", {}, el("span", { class: "person" }, avatar(p.username), el("span", { class: "person-name" }, el("b", {}, p.username),
          el("span", { class: "badges" }, p.role && p.role !== "player" ? badge(roleLabel(p.role), p.role === "admin" ? "danger" : "info", "shield") : null, p.vip ? badge("VIP", "accent", "star") : null, p.guest ? badge(t("Guest")) : null)))),
        el("td", {}, doing(p)),
        el("td", {}, rulesBadge(p.rules)),
        el("td", { class: "num" }, p.guest ? el("span", { class: "faint" }, "—") : fmtNum(recordOf(p, p.rules).rating)), el("td", {}, country(p.country))), () => go("players/" + encodeURIComponent(p.username))));
    };
    const holder = el("div");
    const draw = () => setKids(holder, d.players.length ? table([t("Player"), t("Doing"), t("Rules"), { label: t("Rating"), cls: "num", }, t("Country")], rows()) : empty("users", t("Nobody online"), null));
    filter.addEventListener("input", () => { setQuery({ who: filter.value.trim() }); draw(); });
    draw();
    body = el("div", {}, el("div", { class: "toolbar" }, filter), card(null, holder));
  } else if (tab === "tournaments") {
    body = d.tournaments.length ? el("div", { class: "stack" }, d.tournaments.map(tournamentCard)) : card(null, empty("trophy", t("No tournaments now"), null));
  } else {
    const rows = d.rooms.map((r) => el("tr", { class: r.finished ? "dim" : "" },
      el("td", { class: "mono faint" }, "#" + r.id),
      el("td", {}, el("span", { class: "row" }, personLink(r.players[0]), el("span", { class: "faint" }, "×"), personLink(r.players[1]))),
      el("td", {}, el("span", { class: "badges" }, rulesBadge(r.rules), r.rated ? badge(t("Rated"), "accent") : badge(t("Casual")), r.tournament ? badge(t("Tournament"), "info", "trophy") : null)),
      el("td", { class: "num" }, fmtNum(r.shots)),
      el("td", {}, r.finished ? badge(t("Finished")) : badge(t("Playing"), "success dot")),
      el("td", { class: "actions-cell" }, btn(t("Watch"), { kind: "sm", icon: "eye", onClick: () => openViewer("/live/" + r.id + "/view") }))));
    body = card(null, rows.length ? table(["", t("Players"), t("Game"), { label: t("Strokes"), cls: "num" }, t("State"), ""], rows) : empty("activity", t("No games right now"), t("Games appear here as soon as they start.")));
  }
  return el("div", {},
    pageHead(t("Live"), t("What happens on the server right now · refreshes every 10 seconds"), updatedLabel(refreshPage, true)),
    el("div", { class: "kpis" },
      kpi({ label: t("Online now"), value: fmtNum(s.online), sub: perRulesText(s.online_by_rules), iconName: "users", tone: "success" }),
      kpi({ label: t("Games now"), value: fmtNum(s.games), sub: perRulesText(gamesByRules(playing)), iconName: "activity", tone: "info" }),
      kpi({ label: t("Open tables"), value: fmtNum(s.tables), iconName: "table" }),
      kpi({ label: t("In the queue"), value: fmtNum(s.queue), iconName: "queue", tone: "warning" }),
      kpi({ label: t("Tournaments"), value: fmtNum(s.tournaments), iconName: "trophy" })),
    el("div", { class: "section" }, tabs, body));
};

/** What a player online is doing, as a badge. */
function doing(p) {
  if (p.status === "available" && p.searching) return badge(t("Looking for a game"), "warning", "search");
  if (p.status === "available" && p.vs_computer) return badge(t("Against the computer"), "info", "target");
  if (p.status === "available" && p.away) return badge(t("Away"), "", "clock");
  return badge({ available: t("In the lounge"), waiting: t("At a table"), playing: t("Playing") }[p.status] || p.status, p.status === "playing" ? "info" : p.status === "waiting" ? "warning" : "success");
}
/** The games of a list under each of the rules. */
const gamesByRules = (rooms) => ({ brazilian: rooms.filter((r) => r.rules !== "official").length, official: rooms.filter((r) => r.rules === "official").length });

function tournamentCard(tm) {
  const state = { registering: [t("Signing up"), "warning"], running: [t("Running"), "success"], finished: [t("Finished"), ""] }[tm.state] || [tm.state, ""];
  const cancel = isAdmin() && tm.state === "registering" ? btn(t("Call off"), { kind: "sm danger-soft", icon: "x", onClick: async () => {
    const ok = await confirmDialog({ title: tf("Call off “{name}”?", { name: tm.name }), text: t("Every entry fee goes back and the players are told. It can't be undone."), confirm: t("Call off") });
    if (ok && await attempt(() => api("/tournaments/" + encodeURIComponent(tm.id) + "/cancel", { method: "POST" }), t("Tournament called off."))) refreshIf("live");
  } }) : null;
  const bracket = tm.rounds && tm.rounds.length ? el("div", { class: "bracket" }, tm.rounds.map((round, k) => el("div", { class: "round" },
    el("div", { class: "faint" }, k === tm.rounds.length - 1 ? t("Final") : tf("Round {n}", { n: k + 1 })),
    round.map((m) => el("div", { class: "match" + (m.live ? " live" : "") }, [0, 1].map((s) => el("div", { class: m.winner === s ? "won" : "" }, el("span", { class: "ellipsis" }, m.players[s] || "—"), m.winner === s ? icon("check") : null))))))) : null;
  const standings = tm.standings && tm.standings.length ? barList(tm.standings.slice(0, 10).map((s) => ({ label: el("span", { class: "ellipsis" }, s.username || s[0]), value: s.points ?? s[1] ?? 0 })), { tone: "success" }) : null;
  return card(el("div", { class: "card-head" },
    el("div", {}, el("h3", { class: "row" }, tm.name, badge(state[0], state[1]), rulesBadge(tm.rules), badge(tm.format === "arena" ? t("Arena") : t("Knockout"), "info"), tm.rated ? badge(t("Rated"), "accent") : null, tm.private ? badge(t("Private"), "", "lock") : null),
      el("div", { class: "sub" }, [tm.host ? tf("by {name}", { name: tm.host }) : t("organised by the server"), tf("{n} of {size} players", { n: tm.players.length, size: tm.size }),
        tm.entry_fee ? tf("entry {n} coins", { n: fmtNum(tm.entry_fee) }) : t("free entry"), tm.starts_at ? tf("starts {when}", { when: ago(tm.starts_at) }) : null].filter(Boolean).join(" · "))),
    cancel),
  el("div", { class: "card-body stack" },
    tm.players.length ? el("div", { class: "item-chips" }, tm.players.map((p) => el("a", { class: "item-chip plain", href: "#/players/" + encodeURIComponent(p.username) }, p.username, el("span", { class: "faint" }, fmtNum(recordOf(p, tm.rules).rating))))) : el("p", { class: "faint" }, t("Nobody has signed up yet.")),
    tm.champion ? alertBox("success", tf("Champion: {name}", { name: tm.champion })) : null,
    bracket, standings));
}

// ====================================================================== chat

PAGES.chat = async (route) => {
  const showDeleted = route.query.get("deleted") !== "0";
  const who = route.query.get("q") || "";
  const d = await api("/chat");
  every(10000, () => refreshIf("chat"));
  const lines = d.lines.slice().reverse().filter((l) => (showDeleted || !l.deleted)
    && (!who || (l.from || "").toLowerCase().includes(who.toLowerCase()) || (l.text || "").toLowerCase().includes(who.toLowerCase())));
  const search = el("input", { class: "input wide", type: "search", placeholder: t("Name or words"), value: who, "aria-label": t("Search the chat"), "data-focus": "chat-q" });
  search.addEventListener("input", debounce(() => { setQuery({ q: search.value.trim() }); render({ quiet: true }); }, 300));
  const toggle = el("label", { class: "check" }, el("input", { type: "checkbox", checked: showDeleted || null, onchange: (e) => { setQuery({ deleted: e.target.checked ? null : "0" }); render(); } }), t("Show deleted lines"));
  const clear = isAdmin() ? btn(t("Clear the chat"), { kind: "danger-soft", icon: "trash", onClick: async () => {
    const ok = await confirmDialog({ title: t("Clear the lounge chat for everyone?"), text: t("Every line disappears from the players' screens (the staff keep them for reports)."), confirm: t("Clear the chat") });
    if (ok && await attempt(() => api("/chat/clear", { method: "POST" }), t("The chat was cleared."))) refreshIf("chat");
  } }) : null;
  const del = async (l, silent) => {
    const ok = await confirmDialog({
      title: silent ? t("Remove this line without a trace?") : t("Delete this line?"),
      text: silent ? t("It disappears for everyone, with no sign it was there.") : t("Players see that the staff deleted it."),
      confirm: silent ? t("Remove") : t("Delete"), details: el("div", { class: "preview-box" }, el("b", {}, l.from + ": "), l.text),
    });
    if (ok && await attempt(() => api("/chat/" + encodeURIComponent(l.id) + "/delete", { method: "POST", body: { silent } }), silent ? t("Line removed.") : t("Line deleted."))) refreshIf("chat");
  };
  const DELETED = { author: t("Deleted by the author"), staff: t("Deleted by the staff"), removed: t("Removed without a trace") };
  const rows = lines.map((l) => {
    const player = l.kind === "player";
    const person = { username: l.from, online: true };
    return el("div", { class: "chat-line " + (l.kind || "") + (l.deleted ? " deleted" : "") },
      el("span", { class: "time", title: fmtDate(l.at) }, fmtClock(l.at)),
      el("div", { class: "msg" },
        player ? el("a", { class: "from" + (l.admin ? " admin" : ""), href: "#/players/" + encodeURIComponent(l.from) }, l.from) : el("b", { class: "from" }, l.from),
        el("span", { class: "text" }, l.text),
        l.deleted ? el("span", {}, " ", badge(DELETED[l.deleted] || l.deleted, l.deleted === "removed" ? "danger" : "warning")) : null),
      player ? el("div", { class: "line-actions" },
        !l.deleted ? btn(t("Delete"), { kind: "sm ghost icon-only", icon: "trash", iconOnly: true, onClick: () => del(l, false) }) : null,
        isAdmin() && l.deleted !== "removed" ? btn(t("Remove without a trace"), { kind: "sm ghost icon-only", icon: "eyeOff", iconOnly: true, onClick: () => del(l, true) }) : null,
        btn(t("Warn"), { kind: "sm ghost icon-only", icon: "warn", iconOnly: true, onClick: () => noteDialog(person, "warning", () => {}) }),
        btn(t("Mute"), { kind: "sm ghost icon-only", icon: "mute", iconOnly: true, onClick: () => muteDialog(person, () => {}) })) : el("span"));
  });
  rows.forEach((r) => r.querySelectorAll(".line-actions .btn").forEach((b) => { b.title = b.getAttribute("aria-label"); }));
  return el("div", {},
    pageHead(t("Lounge chat"), t("The latest lines, newest first · refreshes every 10 seconds"), updatedLabel(refreshPage, true), clear),
    el("div", { class: "toolbar" }, search, toggle),
    card(null, rows.length ? el("div", { class: "chat-log" }, rows) : empty("chat", t("Nothing said yet"), who ? t("No line matches.") : t("The lounge chat is quiet."))));
};

// ================================================================= fair play

PAGES.fairplay = async () => {
  const d = await api("/aim");
  const rows = d.flagged.map((r) => clickable(el("tr", { class: "click" },
    el("td", {}, personLink(r.username)),
    el("td", { class: "num" }, fmtNum(r.rating), el("div", { class: "faint" }, tierLabel(r.tier))),
    el("td", { class: "num" }, tf("{made} of {attempts}", { made: fmtNum(r.made), attempts: fmtNum(r.attempts) })),
    el("td", { class: "num" }, el("b", {}, fmtPct(r.rate * 100, 0))),
    el("td", { class: "num" }, fmtPct(r.tier_mean * 100, 0), el("div", { class: "faint" }, tn(r.tier_players, "{n} player", "{n} players"))),
    el("td", { class: "num" }, badge("+" + r.z.toFixed(1) + "σ", r.z >= 5 ? "danger" : "warning"))), () => go("players/" + encodeURIComponent(r.username))));
  return el("div", {},
    pageHead(t("Fair play"), t("Players who make far more hard shots than their tier")),
    alertBox("info", t("How it works"), tf("Hard shots are cuts of 45° or more over 1.5 m, under either of the rules. Only players with {n} or more of them are compared, each with the players of their tier (by the higher of their two ratings). Nobody is punished by this list: open the player, watch their games, and decide.", { n: d.min_attempts })),
    el("div", { class: "kpis section" },
      kpi({ label: t("Players compared"), value: fmtNum(d.players), iconName: "users" }),
      kpi({ label: t("Standing out"), value: fmtNum(d.flagged.length), iconName: "target", tone: d.flagged.length ? "danger" : "success" })),
    el("div", { class: "section" }, card(null, rows.length ? table([t("Player"), { label: t("Higher rating"), cls: "num" }, { label: t("Hard shots made"), cls: "num" }, { label: t("Rate"), cls: "num" }, { label: t("Tier average"), cls: "num" }, { label: t("Above the tier"), cls: "num" }], rows)
      : empty("checkCircle", t("Nobody stands out"), t("No player's hard shots are out of line with their tier.")))));
};

// ================================================================= community

const NOTE_KINDS = [["notice", "Notice"], ["event", "Event"], ["rule", "Rule"]];
const NOTE_BUTTONS = [["", "No button"], ["tournaments", "Tournaments"], ["shop", "Shop"], ["vip", "VIP"], ["missions", "Missions"]];

PAGES.community = async () => {
  const [o, board, seeks, sent] = await Promise.all([api("/overview"), api("/board"), api("/seeks"), api("/audit?action=announce")]);
  // Announcement.
  // data-keep: drafts survive the page refreshing after another action.
  const msg = el("textarea", { class: "textarea", rows: 3, maxlength: "200", placeholder: t("A message to everyone in the lounge, now"), "aria-label": t("Announcement"), id: newId("announce"), "data-keep": "announce" });
  const counter = el("span", { class: "counter" }, "0 / 200");
  msg.addEventListener("input", () => { counter.textContent = fmtNum([...msg.value].length) + " / 200"; });
  const send = btn(t("Send to the lounge"), { kind: "primary", icon: "send", onClick: async () => {
    const text = oneLine(msg.value).trim();
    if (!text) { toast(t("Write a message."), "error"); msg.focus(); return; }
    const ok = await confirmDialog({ title: t("Send this announcement?"), text: t("Everyone in the lounge sees it in the chat, highlighted."), confirm: t("Send"), tone: "info", iconName: "megaphone",
      details: el("div", { class: "preview-box" }, el("span", { class: "faint" }, t("Announcement")), text) });
    if (ok && await busy(send, () => attempt(() => api("/announce", { method: "POST", body: { text } }), t("Announcement sent.")))) { msg.value = ""; refreshIf("community"); }
  } });
  // The board.
  const noteText = el("textarea", { class: "textarea", rows: 3, maxlength: "400", placeholder: t("A note that stays on the lounge's board: an event, a rule, news"), "aria-label": t("Note"), id: newId("board"), "data-keep": "board-note" });
  const noteCount = el("span", { class: "counter" }, "0 / 400");
  let kind = "notice";
  const kindSel = el("div");
  const buttonSel = el("select", { class: "select", "aria-label": t("Button") }, NOTE_BUTTONS.map(([v, l]) => el("option", { value: v }, v ? tf("Button: {name}", { name: t(l) }) : t(l))));
  const daysSel = el("select", { class: "select", "aria-label": t("Up for") }, [["", t("Until taken down")], ["1", tn(1, "{n} day", "{n} days")], ["3", tn(3, "{n} day", "{n} days")], ["7", tn(7, "{n} day", "{n} days")], ["14", tn(14, "{n} day", "{n} days")], ["30", tn(30, "{n} day", "{n} days")]].map(([v, l]) => el("option", { value: v }, l)));
  const pinned = el("input", { type: "checkbox" });
  const preview = el("div");
  const drawPreview = () => {
    noteCount.textContent = fmtNum([...noteText.value].length) + " / 400";
    // Rebuilt on each change: the keyboard stays on the chosen kind.
    const kept = kindSel.contains(document.activeElement);
    setKids(kindSel, segmented(NOTE_KINDS.map(([v, l]) => [v, t(l)]), kind, (v) => { kind = v; drawPreview(); }, t("Kind of note")));
    if (kept) $("button.on", kindSel).focus();
    setKids(preview, el("div", { class: "board-note " + kind },
      el("div", { class: "bn-meta" }, pinned.checked ? badge(t("Pinned"), "accent", "pin") : null, badge(t(NOTE_KINDS.find((k) => k[0] === kind)[1]))),
      el("div", { class: "bn-text" }, oneLine(noteText.value) || el("span", { class: "faint" }, t("The note shows here as players see it."))),
      buttonSel.value ? el("span", { class: "btn sm bn-button" }, t(NOTE_BUTTONS.find((b) => b[0] === buttonSel.value)[1])) : null));
  };
  for (const x of [noteText, buttonSel, pinned]) x.addEventListener(x === noteText ? "input" : "change", drawPreview);
  drawPreview();
  const pin = btn(t("Put on the board"), { kind: "primary", icon: "pin", onClick: () => busy(pin, async () => {
    const text = oneLine(noteText.value).trim();
    if (!text) { toast(t("Write a note."), "error"); noteText.focus(); return; }
    const body = { text, days: daysSel.value ? Number(daysSel.value) : null, kind, action: buttonSel.value || null, pinned: pinned.checked };
    if (await attempt(() => api("/board", { method: "POST", body }), t("On the board."))) { noteText.value = ""; refreshIf("community"); }
  }) });
  const notes = board.notes.length ? el("div", { class: "stack" }, board.notes.map((n) => el("div", { class: "board-note " + n.kind },
    el("div", { class: "bn-head" },
      el("div", { class: "bn-meta" }, n.pinned ? badge(t("Pinned"), "accent", "pin") : null, badge(t((NOTE_KINDS.find((k) => k[0] === n.kind) || [0, n.kind])[1])),
        n.action ? badge(tf("Button: {name}", { name: t((NOTE_BUTTONS.find((b) => b[0] === n.action) || [0, n.action])[1]) })) : null),
      btn(t("Take down"), { kind: "sm ghost", icon: "trash", onClick: async () => {
        if (await confirmDialog({ title: t("Take this note down?"), confirm: t("Take down"), details: el("div", { class: "preview-box" }, n.text) })
          && await attempt(() => api("/board/" + encodeURIComponent(n.id), { method: "DELETE" }), t("Taken down."))) refreshIf("community");
      } })),
    el("div", { class: "bn-text" }, n.text),
    el("div", { class: "bn-meta" }, el("span", {}, tf("by {name}", { name: n.author || "?" }), " · ", timeEl(n.created_at)),
      n.expires_at ? el("span", {}, "· " + tf("until {when}", { when: fmtDate(n.expires_at) })) : el("span", {}, "· " + t("until taken down")))))) : empty("pin", t("The board is empty"), t("Notes put here stay in the lounge for everyone."));
  // Welcome message.
  const motd = el("input", { class: "input", maxlength: "300", value: o.motd, "aria-label": t("Welcome message"), disabled: isAdmin() ? null : true });
  const saveMotd = btn(t("Save"), { kind: "primary", onClick: () => busy(saveMotd, () => attempt(() => api("/motd", { method: "POST", body: { text: motd.value } }), t("Welcome message saved."))) });
  // "Looking for a game".
  const seekRows = seeks.notes.map((n) => el("tr", {},
    el("td", {}, personLink(n.username)), el("td", { class: "wrap" }, n.text || el("span", { class: "faint" }, t("(no message)"))),
    el("td", {}, n.rated ? badge(t("Rated"), "accent") : badge(t("Casual"))), el("td", {}, timeEl(n.at)),
    el("td", { class: "actions-cell" }, btn(t("Remove"), { kind: "sm ghost", icon: "trash", onClick: async () => {
      if (await confirmDialog({ title: tf("Remove {name}'s note?", { name: n.username }), confirm: t("Remove") })
        && await attempt(() => api("/seeks/" + encodeURIComponent(n.username), { method: "DELETE" }), t("Removed."))) refreshIf("community");
    } }))));
  return el("div", {},
    pageHead(t("Board & announcements"), t("What players read in the lounge")),
    el("div", { class: "grid-2" },
      el("div", { class: "stack" },
        card(cardHead(t("Announcement"), t("Shown at once in the lounge chat, highlighted")),
          el("div", { class: "card-body stack" }, el("div", { class: "field" }, el("label", { class: "label", for: msg.id }, el("span", {}, t("Message")), counter), msg)),
          el("div", { class: "card-foot" }, send)),
        card(cardHead(t("Sent lately")), sent.entries.length ? el("div", { class: "mini-list" }, sent.entries.slice(0, 8).map((e) => el("div", { class: "mi" },
          el("span", { class: "tl-dot info" }, icon("megaphone")), el("div", { class: "mi-main" }, el("span", {}, e.details), el("span", { class: "mi-sub" }, tf("by {name}", { name: e.staff }))), el("span", { class: "mi-side" }, timeEl(e.at)))))
          : empty("megaphone", t("No announcements yet"), null)),
        card(cardHead(t("Welcome message"), t("Shown to everyone who signs in · until the server restarts")),
          el("div", { class: "card-body" }, motd),
          isAdmin() ? el("div", { class: "card-foot" }, saveMotd) : null)),
      el("div", { class: "stack" },
        card(cardHead(t("The lounge's board"), t("Notes that stay up")),
          el("div", { class: "card-body stack" },
            el("div", { class: "field" }, el("label", { class: "label", for: noteText.id }, el("span", {}, t("Note")), noteCount), noteText),
            el("div", { class: "row" }, kindSel, buttonSel, daysSel),
            el("label", { class: "check" }, pinned, t("Pinned at the top")),
            el("div", { class: "field" }, el("div", { class: "label" }, t("Preview")), preview)),
          el("div", { class: "card-foot" }, pin)),
        card(cardHead(t("On the board now"), tn(board.notes.length, "{n} note", "{n} notes")), el("div", { class: "card-body" }, notes)))),
    el("div", { class: "section" }, card(cardHead(t("Looking for a game"), t("Players' notes, up while they are online")),
      seekRows.length ? table([t("Player"), t("Note"), t("Game"), t("Posted"), ""], seekRows) : empty("users", t("Nobody is looking for a game"), null))));
};

// ==================================================================== events

const EVENT_KINDS = { xp: ["Experience", "star", "success"], coins: ["Coins from games", "coins", "warning"], tournament: ["Themed tournament", "trophy", "info"] };
const EVENT_STATUS = { live: ["Running", "success dot"], scheduled: ["Scheduled", "info"], ended: ["Ended", ""], cancelled: ["Called off", "danger"] };

/** What an event does, in a line. */
function eventWhat(e) {
  if (e.kind === "tournament") {
    return [rulesName(e.rules), e.format === "arena" ? tf("Arena · {time}", { time: fmtMinutes(Math.round((e.ends_at - e.starts_at) / 60)) }) : tf("Knockout · {n} seats", { n: e.size }),
      e.rated ? t("rated") : t("casual"), e.entry_fee ? tf("entry {n} coins", { n: fmtNum(e.entry_fee) }) : t("free entry"),
      e.prize ? tf("the house adds {n} coins", { n: fmtNum(e.prize) }) : null].filter(Boolean).join(" · ");
  }
  return tf("{what} ×{n}", { what: t((EVENT_KINDS[e.kind] || [e.kind])[0]), n: e.multiplier });
}

PAGES.events = async () => {
  const d = await api("/events");
  every(30000, () => refreshIf("events"));
  const now = d.now;
  const live = d.events.filter((e) => e.status === "live").sort((a, b) => a.ends_at - b.ends_at);
  const next = d.events.filter((e) => e.status === "scheduled").sort((a, b) => a.starts_at - b.starts_at);
  const past = d.events.filter((e) => e.status === "ended" || e.status === "cancelled").slice(0, 20);
  const boostNow = (kind) => Math.max(1, ...live.filter((e) => e.kind === kind).map((e) => e.multiplier));
  const section = (title, sub, list, emptyTitle, emptyText) => el("div", { class: "section" },
    el("div", { class: "section-head" }, el("h2", {}, title), sub ? el("span", { class: "sub" }, sub) : null),
    list.length ? el("div", { class: "stack" }, list.map((e) => eventCard(e, now))) : card(null, empty("calendar", emptyTitle, emptyText)));
  return el("div", {},
    pageHead(t("Events"), t("Experience or coins that count more for a while, and themed tournaments"),
      isAdmin() ? btn(t("New event"), { kind: "primary", icon: "calendar", onClick: () => eventDialog(d.limits) }) : null),
    isAdmin() ? null : alertBox("info", t("Only administrators schedule events"), t("They change what the game pays out.")),
    el("div", { class: "kpis" },
      kpi({ label: t("Experience now"), value: "×" + boostNow("xp"), sub: boostNow("xp") > 1 ? t("a boost is running") : t("as usual"), iconName: "star", tone: boostNow("xp") > 1 ? "success" : "" }),
      kpi({ label: t("Coins from games now"), value: "×" + boostNow("coins"), sub: boostNow("coins") > 1 ? t("a boost is running") : t("as usual"), iconName: "coins", tone: boostNow("coins") > 1 ? "warning" : "" }),
      kpi({ label: t("Coming up"), value: fmtNum(next.length), sub: next.length ? tf("the next {when}", { when: ago(next[0].starts_at) }) : t("nothing scheduled"), iconName: "calendar", tone: "info" })),
    section(t("Running now"), null, live, t("No event is running"), isAdmin() ? t("Schedule one with “New event”.") : null),
    section(t("Coming up"), t("Players see what starts within two weeks"), next, t("Nothing scheduled"), null),
    past.length ? section(t("Past"), t("The latest 20"), past, "", null) : null);
};

function eventCard(e, now) {
  const [, ic, tone] = EVENT_KINDS[e.kind] || [e.kind, "calendar", ""];
  const [st, stTone] = EVENT_STATUS[e.status] || [e.status, ""];
  let progress = null;
  if (e.status === "live") {
    const fill = el("div", { class: "bl-fill " + tone });
    fill.style.width = Math.max(1, Math.min(100, ((now - e.starts_at) * 100) / Math.max(1, e.ends_at - e.starts_at))) + "%";
    progress = el("div", { class: "bl-track" }, fill);
  }
  const when = e.status === "live" ? tf("ends {when}", { when: ago(e.ends_at) })
    : e.status === "scheduled" ? tf("starts {when}", { when: ago(e.starts_at) })
      : e.status === "cancelled" ? tf("called off {when}", { when: ago(e.cancelled_at) }) : tf("ended {when}", { when: ago(e.ends_at) });
  const cancel = isAdmin() && (e.status === "live" || e.status === "scheduled") ? btn(t("Call off the event"), { kind: "sm danger-soft", icon: "x", onClick: async () => {
    const ok = await confirmDialog({
      title: tf("Call off the event “{name}”?", { name: e.title }),
      text: e.kind === "tournament" ? t("If its tournament is open for sign-up, it is called off and every entry fee goes back.") : t("The boost stops at once."),
      confirm: t("Call off the event"),
    });
    if (ok && await attempt(() => api("/events/" + encodeURIComponent(e.id) + "/cancel", { method: "POST" }), t("Event called off."))) refreshIf("events");
  } }) : null;
  return el("section", { class: "card" + (e.status === "ended" || e.status === "cancelled" ? " faded" : "") },
    el("div", { class: "card-head" },
      el("div", { class: "row" }, el("span", { class: "event-icon " + tone }, icon(ic)),
        el("div", {}, el("h3", {}, e.title), el("div", { class: "sub" }, eventWhat(e)))),
      el("div", { class: "row" }, badge(t(st), stTone), cancel)),
    el("div", { class: "card-body stack" },
      el("div", { class: "row between" }, el("span", { class: "num" }, fmtDate(e.starts_at) + " → " + fmtDate(e.ends_at)), el("span", { class: "muted" }, when)),
      progress,
      el("div", { class: "faint" }, tf("Scheduled by {name}, {when}", { name: e.created_by || "?", when: ago(e.created_at) }))));
}

const pad2 = (n) => String(n).padStart(2, "0");
/** A time for a datetime-local field (the viewer's time zone). */
const toLocalInput = (secs) => {
  const d = new Date(secs * 1000);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}T${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
};
const fromLocalInput = (v) => (v ? Math.floor(new Date(v).getTime() / 1000) : NaN);
/** The viewer's time zone, as "UTC−03:00". */
function zoneLabel() {
  const off = -new Date().getTimezoneOffset();
  return "UTC" + (off >= 0 ? "+" : "−") + pad2(Math.floor(Math.abs(off) / 60)) + ":" + pad2(Math.abs(off) % 60);
}
/** The next `weekday` (0 Sunday … 6 Saturday) at `hour`:`minute`, local,
 * strictly after `from`. */
function nextAt(weekday, hour, minute, from = new Date()) {
  const d = new Date(from);
  d.setHours(hour, minute, 0, 0);
  let add = (weekday - d.getDay() + 7) % 7;
  if (add === 0 && d <= from) add = 7;
  d.setDate(d.getDate() + add);
  return d;
}

function eventDialog(limits) {
  let kind = "xp", mult = 2, format = "arena", size = 8, rules = "brazilian";
  // A field's name: a real label for an input. A row of buttons is named
  // where it is built (segmented's label, the chips' aria-label).
  const field = (label, input, hint) => {
    const control = ["INPUT", "SELECT", "TEXTAREA"].includes(input.tagName);
    if (control && !input.id) input.id = newId("event");
    return el("div", { class: "field" }, el(control ? "label" : "div", { class: "label", for: control ? input.id : null }, label), input, hint ? el("div", { class: "hint" }, hint) : null);
  };
  const kindBox = el("div"), multBox = el("div"), formatBox = el("div"), sizeBox = el("div"), rulesBox = el("div");
  const title = el("input", { class: "input", id: newId("event"), placeholder: t("Double XP weekend"), "aria-label": t("Title") });
  const titleName = el("span", {}, t("Title"));
  const count = el("span", { class: "counter" });
  const starts = el("input", { class: "input", type: "datetime-local", "aria-label": t("Starts") });
  const ends = el("input", { class: "input", type: "datetime-local", "aria-label": t("Ends") });
  const presets = el("div", { class: "chips", role: "group", "aria-label": t("Quick picks") });
  const rated = el("input", { type: "checkbox" });
  rated.checked = true;
  const fee = el("input", { class: "input", type: "number", min: "0", max: String(limits.max_entry_fee), value: "0", step: "10" });
  const prize = el("input", { class: "input", type: "number", min: "0", max: String(limits.max_prize), value: "0", step: "100" });
  const sizeField = field(t("Seats"), sizeBox);
  const boostFields = el("div", { class: "stack" }, field(t("Multiplier"), multBox, t("Boosts don't add up: the largest one running counts.")));
  const tourFields = el("div", { class: "stack" }, field(t("Rules"), rulesBox, t("Every game of the tournament is played under these rules; a rated one counts for their rating.")), field(t("Format"), formatBox), sizeField,
    el("label", { class: "check" }, rated, t("Rated")),
    el("div", { class: "grid-2" }, field(t("Entry fee (coins)"), fee), field(t("Prize from the house (coins)"), prize)),
    el("div", { class: "hint" }, t("The house's prize goes 70% to the champion and 30% to the runner-up, on top of the entry fees. The tournament opens for sign-up two hours before it starts, in Tournaments.")));
  const summary = el("div", { class: "preview-box" });
  const errorBox = el("div", { class: "alert danger hidden", role: "alert" });
  const setTimes = (a, b) => { starts.value = toLocalInput(a); ends.value = toLocalInput(b); draw(); };
  const now = () => Math.floor(Date.now() / 60000) * 60;
  // Two minutes ahead: a tournament must start at least a minute from now.
  const soon = () => new Date(Date.now() + 120000);
  // Each change rebuilds the rows of buttons: the keyboard goes back to the
  // chosen one (or the same chip), not to the page behind the dialog.
  const refill = (box, node) => {
    const at = [...box.querySelectorAll("button")].indexOf(document.activeElement);
    setKids(box, node);
    if (at < 0) return;
    const again = box.querySelector("button.on") || box.querySelectorAll("button")[at];
    if (again) again.focus();
  };
  function draw() {
    refill(kindBox, segmented([["xp", t("Experience")], ["coins", t("Coins from games")], ["tournament", t("Themed tournament")]], kind, (v) => { kind = v; presetDefaults(); draw(); }, t("Kind of event")));
    refill(multBox, segmented([[2, "×2"], [3, "×3"]].slice(0, limits.max_multiplier - 1), mult, (v) => { mult = v; draw(); }, t("Multiplier")));
    refill(rulesBox, segmented(RULES.map((r) => [r, rulesName(r)]), rules, (v) => { rules = v; draw(); }, t("Rules")));
    refill(formatBox, segmented([["arena", t("Arena")], ["knockout", t("Knockout")]], format, (v) => { format = v; presetDefaults(true); draw(); }, t("Format")));
    refill(sizeBox, segmented(limits.knockout_sizes.map((n) => [n, String(n)]), size, (v) => { size = v; draw(); }, t("Seats")));
    boostFields.classList.toggle("hidden", kind === "tournament");
    tourFields.classList.toggle("hidden", kind !== "tournament");
    sizeField.classList.toggle("hidden", format !== "knockout");
    count.textContent = fmtNum([...title.value].length) + " / " + fmtNum(limits.max_title);
    count.classList.toggle("over", [...title.value].length > limits.max_title);
    const a = fromLocalInput(starts.value), b = fromLocalInput(ends.value);
    const e = { kind, title: title.value || title.placeholder, multiplier: mult, format, size, rules, rated: rated.checked, entry_fee: Number(fee.value) || 0, prize: Number(prize.value) || 0, starts_at: a, ends_at: b };
    setKids(summary, el("span", { class: "faint" }, t("Players see")),
      el("b", {}, e.title), el("div", {}, eventWhat(e)),
      a && b && b > a ? el("div", { class: "muted" }, tf("{from} → {to} ({length})", { from: fmtDate(a), to: fmtDate(b), length: fmtDuration(b - a) })) : null);
    refill(presets, (kind === "tournament"
      ? [[t("Tonight at 8 pm"), () => { const d = new Date(); d.setHours(20, 0, 0, 0); if (d < soon()) d.setDate(d.getDate() + 1); return d; }],
        [t("Saturday at 8 pm"), () => nextAt(6, 20, 0, soon())], [t("Sunday at 4 pm"), () => nextAt(0, 16, 0, soon())]]
        .map(([label, at]) => el("button", { type: "button", class: "chip", onclick: () => { const s0 = Math.floor(at().getTime() / 1000); setTimes(s0, s0 + (format === "arena" ? 3600 : 3 * 3600)); } }, label))
      : [[t("Now, for 24 hours"), () => setTimes(now(), now() + 86400)],
        [t("This weekend"), () => {
          const day = new Date().getDay();
          const from = day === 6 || day === 0 ? now() : Math.floor(nextAt(6, 0, 0).getTime() / 1000);
          // Until Monday 00:00, so the last minute of Sunday counts too.
          setTimes(from, Math.floor(nextAt(1, 0, 0, new Date(from * 1000)).getTime() / 1000));
        }],
        [t("The next 7 days"), () => setTimes(now(), now() + 7 * 86400)]]
        .map(([label, go]) => el("button", { type: "button", class: "chip", onclick: go }, label))));
  }
  function presetDefaults(keepStart) {
    const a = fromLocalInput(starts.value);
    if (kind === "tournament") {
      const s0 = keepStart && a ? a : Math.floor(nextAt(6, 20, 0, soon()).getTime() / 1000);
      setTimes(s0, s0 + (format === "arena" ? 3600 : 3 * 3600));
    } else if (!keepStart) setTimes(now(), now() + 86400);
  }
  /** Whole coins from 0 to `max` (empty is 0), or null. */
  const coins = (input, max) => {
    if (input.validity && input.validity.badInput) return null;
    const v = input.value.trim() === "" ? 0 : Number(input.value);
    return Number.isInteger(v) && v >= 0 && v <= max ? v : null;
  };
  for (const x of [title, starts, ends, fee, prize]) x.addEventListener("input", draw);
  rated.addEventListener("change", draw);
  presetDefaults(false);
  dialog({
    title: t("New event"), text: tf("Times in your time zone ({zone}).", { zone: zoneLabel() }), iconName: "calendar", wide: true,
    body: [field(t("Kind of event"), kindBox),
      el("div", { class: "field" }, el("label", { class: "label", for: title.id }, titleName, count), title,
        el("div", { class: "hint" }, t("Players see it as written (in one language)."))),
      field(t("Quick picks"), presets),
      el("div", { class: "grid-2" }, field(t("Starts"), starts), field(t("Ends"), ends)),
      boostFields, tourFields, summary, errorBox],
    actions: [
      { label: t("Cancel"), onClick: (h) => h.close(false) },
      { label: t("Schedule"), kind: "primary", primary: true, onClick: async (h, button) => {
        errorBox.classList.add("hidden");
        let a = fromLocalInput(starts.value);
        const b = fromLocalInput(ends.value);
        // The form may have stood open a while: a boost that should have
        // started already starts now.
        if (kind !== "tournament" && a && a <= nowSecs()) a = Math.floor(nowSecs());
        const tour = kind === "tournament";
        const entryFee = coins(fee, limits.max_entry_fee), housePrize = coins(prize, limits.max_prize);
        let problem = null, where = null;
        if (!title.value.trim()) problem = t("Give the event a title.");
        else if ([...title.value].length > limits.max_title) problem = tf("“{field}” is too long.", { field: t("Title") });
        else if (!(a && b)) problem = t("Choose when it starts and ends.");
        else if (b <= a) problem = t("The event must end after it starts.");
        else if (tour && entryFee === null) { problem = tf("The entry fee must be a whole number from 0 to {max}.", { max: fmtNum(limits.max_entry_fee) }); where = fee; }
        else if (tour && housePrize === null) { problem = tf("The house's prize must be a whole number from 0 to {max}.", { max: fmtNum(limits.max_prize) }); where = prize; }
        fee.classList.toggle("invalid", where === fee);
        prize.classList.toggle("invalid", where === prize);
        if (problem) {
          setKids(errorBox, icon("warn"), el("div", {}, problem));
          errorBox.classList.remove("hidden");
          if (where) where.focus();
          return;
        }
        const body = { kind, title: title.value.trim(), multiplier: mult, starts_at: a, ends_at: b, format, size, rules, rated: rated.checked, entry_fee: tour ? entryFee : 0, prize: tour ? housePrize : 0 };
        const label = button.textContent;
        button.disabled = true;
        setKids(button, el("span", { class: "spinner" }), label);
        try {
          await api("/events", { method: "POST", body });
          toast(t("Event scheduled."), "success");
          h.close(true);
          refreshIf("events");
        } catch (err) {
          setKids(errorBox, icon("warn"), el("div", {}, err.message));
          errorBox.classList.remove("hidden");
        } finally {
          button.disabled = false;
          button.textContent = label;
        }
      } },
    ],
  });
  // The title's label follows the kind.
  const syncLabel = () => {
    const name = kind === "tournament" ? t("Name") : t("Title");
    titleName.textContent = name;
    title.setAttribute("aria-label", name);
    title.placeholder = kind === "tournament" ? t("Halloween Cup") : kind === "coins" ? t("Coins weekend") : t("Double XP weekend");
  };
  kindBox.addEventListener("click", () => setTimeout(() => { syncLabel(); draw(); }, 0));
  syncLabel();
}

// =================================================================== economy

const CATEGORY = { cloth: "Cloth", cue: "Cue", chalk: "Chalk", balls: "Balls", frame: "Frame", name: "Name colour", effect: "Win effect", table: "Table", hall: "Hall", avatar: "Picture" };
const HOW = { coins: ["Coins", "accent"], free: ["Free", ""], achievement: ["Achievement", "info"], vip: ["VIP", "warning"], pack: ["Welcome pack", "success"] };
const FLOW = { buy: "Bought in the shop", gift: "Gifts to friends", daily: "Daily bonus and challenge", game: "Games played", league: "Leagues reached", level: "Levels reached",
  staff: "Given by the staff", refund: "Refunds", tournament: "Tournament entries", prize: "Tournament prizes", mission: "Missions", pack: "Coin packs bought", welcome: "Welcome pack", vip: "VIP" };

PAGES.economy = async (route) => {
  const tab = route.query.get("tab") || "coins";
  const [e, m] = await Promise.all([api("/economy"), api("/metrics")]);
  catalogCache = e.catalog;
  const eco = e.economy;
  const revenue = (m.metrics.revenue || []).filter((r) => r[0] !== "msstore");
  const ms = (m.metrics.revenue || []).find((r) => r[0] === "msstore");
  const flows = eco.flows.map((f) => el("tr", {},
    el("td", {}, el("b", {}, t(FLOW[f.kind] || f.kind)), el("div", { class: "faint mono" }, f.kind)),
    el("td", { class: "num" }, f.credited ? el("span", { class: "badge success" }, "+" + fmtNum(f.credited)) : "—"),
    el("td", { class: "num" }, f.debited ? el("span", { class: "badge danger" }, "−" + fmtNum(f.debited)) : "—"),
    el("td", { class: "num" }, fmtNum(f.entries))));
  const credited = eco.flows.reduce((a, f) => a + f.credited, 0), debited = eco.flows.reduce((a, f) => a + f.debited, 0);
  const pays = e.payments.map((p) => el("tr", { class: p.test || p.refunded ? "dim" : "" },
    el("td", {}, fmtDate(p.at)), el("td", {}, personLink(p.payer)), el("td", {}, p.kind === "vip" ? "VIP" : t("Coins")),
    el("td", { class: "num" }, fmtNum(p.coins)), el("td", { class: "num" }, money(p.amount, p.currency)),
    el("td", {}, el("span", { class: "badges" }, p.refunded ? badge(t("Refunded"), "danger") : badge(t("Paid"), "success"), p.test ? badge(t("Test"), "warning") : null)),
    el("td", { class: "actions-cell" }, isAdmin() ? btn(p.test ? t("Mark as real") : t("Mark as test"), { kind: "sm ghost", onClick: () => markPayment(p, () => refreshIf("economy")) }) : null)));
  // The shop's catalogue.
  const q = el("input", { class: "input wide", type: "search", placeholder: t("Item name or code"), "aria-label": t("Search items") });
  const cats = [...new Set(e.catalog.map((i) => i.category))];
  const catSel = el("select", { class: "select", "aria-label": t("Category") }, el("option", { value: "" }, t("Every category")), cats.map((c) => el("option", { value: c }, t(CATEGORY[c] || c))));
  const howSel = el("select", { class: "select", "aria-label": t("How it is had") }, el("option", { value: "" }, t("Any way")), Object.entries(HOW).map(([v, [l]]) => el("option", { value: v }, t(l))));
  const catalogBox = el("div");
  let shownItems = 40;
  const drawCatalog = () => {
    const s = q.value.trim().toLowerCase();
    const all = e.catalog.filter((i) => (!s || i.name.toLowerCase().includes(s) || i.code.includes(s)) && (!catSel.value || i.category === catSel.value) && (!howSel.value || i.how === howSel.value));
    const items = all.slice(0, shownItems);
    const more = all.length > items.length ? el("div", { class: "pager" }, el("span", {}, tf("{from}–{to} of {total}", { from: 1, to: fmtNum(items.length), total: fmtNum(all.length) })),
      btn(tf("Show all {n}", { n: fmtNum(all.length) }), { kind: "sm", onClick: () => { shownItems = Infinity; drawCatalog(); } })) : null;
    setKids(catalogBox, items.length ? [table([t("Item"), t("Category"), t("How it is had"), { label: t("Price"), cls: "num" }], items.map((i) => el("tr", {},
      el("td", {}, el("b", {}, i.name), el("div", { class: "faint mono" }, i.code)), el("td", {}, t(CATEGORY[i.category] || i.category)),
      el("td", {}, badge(t((HOW[i.how] || [i.how])[0]), (HOW[i.how] || ["", ""])[1]), i.achievement ? el("span", { class: "faint mono" }, " " + i.achievement) : null),
      el("td", { class: "num" }, i.coins ? fmtNum(i.coins) : "—")))), more] : empty("search", t("No item matches"), null));
  };
  for (const x of [q, catSel, howSel]) x.addEventListener(x === q ? "input" : "change", () => { shownItems = 40; drawCatalog(); });
  drawCatalog();
  return el("div", {},
    pageHead(t("Economy"), t("Coins, VIP, payments and the shop")),
    el("div", { class: "kpis" },
      kpi({ label: t("Coins in wallets"), value: fmtNum(eco.coins_total), sub: tn(eco.wallets, "{n} wallet with coins", "{n} wallets with coins"), iconName: "coins", tone: "warning" }),
      kpi({ label: t("VIP members"), value: fmtNum(eco.vip_now), iconName: "star", tone: "success" }),
      kpi({ label: t("Given in 30 days"), value: "+" + fmtNum(credited), sub: t("coins credited"), iconName: "gift", tone: "success" }),
      kpi({ label: t("Spent in 30 days"), value: "−" + fmtNum(debited), sub: t("coins taken"), iconName: "coins", tone: "danger" }),
      ...revenue.map((r) => kpi({ label: tf("Revenue in 30 days ({currency})", { currency: r[0].toUpperCase() }), value: money(r[1], r[0]), sub: tn(r[2], "{n} payment", "{n} payments"), iconName: "chart", tone: "info" })),
      ms ? kpi({ label: "Microsoft Store", value: fmtNum(ms[2]), sub: t("purchases in 30 days"), iconName: "coins" }) : null),
    !e.stripe ? el("div", { class: "section" }, alertBox("info", t("Card payments are off"), t("Stripe is not configured on this server."))) : null,
    el("div", { class: "section" }, el("div", { class: "tabs" }, [["coins", t("Coins"), null], ["payments", t("Payments"), e.private_hidden ? null : e.payments.length], ["catalog", t("The shop's catalogue"), e.catalog.length]]
      .map(([id, label, n]) => el("button", { type: "button", class: "tab" + (tab === id ? " on" : ""), onclick: () => { setQuery({ tab: id }); render(); } }, label, n !== null ? el("span", { class: "count" }, fmtNum(n)) : null))),
    tab === "payments"
      ? card(cardHead(t("Latest payments"), t("Test and refunded payments are faded")), e.private_hidden ? empty("lock", t("Visible to administrators only"), null) : pays.length ? table([t("When"), t("Player"), t("What"), { label: t("Coins"), cls: "num" }, { label: t("Amount"), cls: "num" }, t("Status"), ""], pays) : empty("coins", t("No payments yet"), null))
      : tab === "catalog"
        ? el("div", {}, el("div", { class: "toolbar" }, q, catSel, howSel), card(null, catalogBox))
        : el("div", { class: "grid-aside" },
          card(cardHead(t("Where coins went in the last 30 days"), t("By kind of reason")), flows.length ? table([t("Reason"), { label: t("Given"), cls: "num" }, { label: t("Taken"), cls: "num" }, { label: t("Entries"), cls: "num" }], flows) : empty("coins", t("No coins moved"), null)),
          card(cardHead(t("Largest balances")), eco.richest.length ? el("div", { class: "card-body" }, barList(eco.richest.map(([n, c]) => ({ label: personLink(n), value: c })), { tone: "" })) : empty("coins", t("No coins anywhere"), null)))));
};

// =================================================================== metrics

PAGES.metrics = async () => {
  const d = await api("/metrics");
  every(60000, () => refreshIf("metrics"));
  const m = d.metrics, q = d.queue;
  const hours = m.hours;
  const hourLabel = (h) => String(new Date(h * 3600000).getHours()).padStart(2, "0") + "h";
  const hourTitle = (h) => new Intl.DateTimeFormat(lang, { weekday: "short", hour: "2-digit", minute: "2-digit" }).format(new Date(h * 3600000));
  const playersChart = hours.some((h) => h.peak_players > 0 || h.active > 0)
    ? chart({ labels: hours.map((h) => hourLabel(h.hour)), titles: hours.map((h) => hourTitle(h.hour)),
      bars: { values: hours.map((h) => h.active), name: t("Players active"), cls: "c-accent" }, line: { values: hours.map((h) => h.peak_players), name: t("Most online at once"), cls: "c-info" }, width: 560, every: 8 })
    : empty("chart", t("No samples yet"), t("The server counts who is online every minute: the first hour shows up shortly."));
  const gamesChart = hours.some((h) => h.games > 0 || h.peak_games > 0)
    ? chart({ labels: hours.map((h) => hourLabel(h.hour)), titles: hours.map((h) => hourTitle(h.hour)),
      bars: { values: hours.map((h) => h.games), name: t("Games finished"), cls: "c-success" }, line: { values: hours.map((h) => h.peak_games), name: t("Most games at once"), cls: "c-info" }, width: 560, every: 8 })
    : empty("activity", t("No games in the last 48 hours"), null);
  const dailyChart = m.days.some((x) => x[1] > 0 || x[2] > 0)
    ? chart({ labels: m.days.map((x) => fmtDay(x[0])), titles: m.days.map((x) => fmtDayLong(x[0])),
      bars: { values: m.days.map((x) => x[1]), name: t("Players active"), cls: "c-accent" }, line: { values: m.days.map((x) => x[2]), name: t("Games"), cls: "c-success" } })
    : empty("chart", t("Nobody played in the last 30 days"), null);
  const today = Math.floor(d.now / 86400);
  const days = Array.from({ length: 30 }, (_, k) => today - 29 + k);
  const currencies = [...new Set(m.revenue_days.map((r) => r[1]))].filter((c) => c !== "msstore").sort();
  const revenueCharts = currencies.map((cur) => {
    const by = new Map(m.revenue_days.filter((r) => r[1] === cur).map((r) => [r[0], r]));
    const total = m.revenue.find((r) => r[0] === cur) || [cur, 0, 0];
    return card(cardHead(cur.toUpperCase(), money(total[1], cur) + " · " + tn(total[2], "{n} payment", "{n} payments")),
      el("div", { class: "card-body" }, chart({ labels: days.map(fmtDay), titles: days.map(fmtDayLong),
        bars: { values: days.map((day) => (by.get(day) || [0, 0, 0])[2] / 100), name: t("Revenue"), cls: "c-success", format: (v) => new Intl.NumberFormat(lang, { maximumFractionDigits: 2 }).format(v) }, height: 190, width: 520 })));
  });
  const recent = q.recent || [];
  const waits = recent.length ? chart({ labels: recent.map(() => ""), titles: recent.map((_, k) => tf("Pairing {n} of the latest {total}", { n: k + 1, total: recent.length })),
    bars: { values: recent, name: t("Wait (seconds)"), cls: "c-accent" }, every: 0, height: 140, unit: " s" })
    : empty("queue", t("Nobody paired yet"), t("Nobody has been paired by “Play now” since the server started."));
  const week = m.days.slice(-7);
  const perDay = week.length ? week.reduce((a, x) => a + x[2], 0) / week.length : 0;
  const f = m.funnel;
  const funnelStep = (label, n) => {
    const fill = el("div", { class: "f-fill" });
    fill.style.width = (f[0] ? Math.max(n ? 2 : 0, n * 100 / f[0]) : 0) + "%";
    return el("div", { class: "f-step" }, el("div", { class: "f-top" }, el("span", {}, label), el("b", {}, fmtNum(n) + (f[0] ? " · " + fmtPct(n * 100 / f[0], 0) : ""))), el("div", { class: "f-track" }, fill));
  };
  const totalActive = (d.countries || []).reduce((a, c) => a + c[1], 0);
  const countries = (d.countries || []).slice(0, 15).map(([c, n]) => ({ label: country(c), value: n }));
  const level = (x) => (x === null || x === undefined ? "h0" : x >= 40 ? "h4" : x >= 25 ? "h3" : x >= 12 ? "h2" : x > 0 ? "h1" : "h0");
  const pctCell = (x) => el("td", { class: "hcell " + level(x) }, x === null || x === undefined ? "—" : fmtPct(x, 0));
  const cohorts = (m.cohorts || []).map((c) => el("tr", {}, el("td", {}, fmtDay(c.first_day) + " – " + fmtDay(c.first_day + 6)), el("td", { class: "num" }, fmtNum(c.accounts)), pctCell(c.d1), pctCell(c.d7), pctCell(c.d30)));
  return el("div", { class: "stack" },
    pageHead(t("Metrics"), t("How many play, how long they stay, what they pay · refreshes every minute"), updatedLabel(refreshPage)),
    d.draining ? alertBox("warning", t("The server is restarting"), t("No new games start; the games in progress move to the next server.")) : null,
    el("div", { class: "kpis" },
      kpi({ label: t("Online now"), value: fmtNum(d.online), sub: perRulesText(d.online_by_rules), iconName: "users", tone: "success" }),
      kpi({ label: t("Games now"), value: fmtNum(d.games), iconName: "activity", tone: "info" }),
      kpi({ label: t("In the queue"), value: fmtNum(q.size), sub: q.size ? perRulesText(gamesByRules(q.waiting)) + " · " + tf("longest {s} s", { s: fmtNum(q.waiting.reduce((a, w) => Math.max(a, w.secs), 0)) }) : t("nobody waiting"), iconName: "queue" }),
      kpi({ label: t("Median wait"), value: tf("{s} s", { s: fmtNum(q.median_wait_secs) }), sub: tf("average {s} s · {n} pairings", { s: fmtNum(Math.round(q.avg_wait_secs)), n: fmtNum(q.pairs) }), iconName: "clock" }),
      kpi({ label: t("Back the next day"), value: fmtPct(m.retention_d1), iconName: "undo", tone: "success" }),
      kpi({ label: t("Back after 7 days"), value: fmtPct(m.retention_d7), iconName: "undo", tone: "success" }),
      kpi({ label: t("Back after 30 days"), value: fmtPct(m.retention_d30), iconName: "undo", tone: "success" }),
      kpi({ label: t("Games per day"), value: new Intl.NumberFormat(lang, { maximumFractionDigits: 1 }).format(perDay), sub: t("average of the last 7 days"), iconName: "trophy", tone: "warning" })),
    card(cardHead(t("Players and games, day by day"), t("The last 30 days")), el("div", { class: "card-body" }, dailyChart)),
    el("div", { class: "grid-2" },
      card(cardHead(t("Players by hour"), t("The last 48 hours")), el("div", { class: "card-body" }, playersChart)),
      card(cardHead(t("Games by hour"), t("The last 48 hours")), el("div", { class: "card-body" }, gamesChart))),
    el("div", { class: "grid-2" },
      card(cardHead(t("From account to player"), t("Accounts made in the last 30 days")), el("div", { class: "card-body funnel" },
        funnelStep(t("Accounts made"), f[0]), funnelStep(t("Played a game"), f[1]), funnelStep(t("Played five games"), f[2]),
        el("p", { class: "faint" }, t("Installs are not counted: the game sends nothing before signing in.")))),
      card(cardHead(t("Where players come from"), tf("Active in the last 30 days · {n} countries", { n: fmtNum((d.countries || []).length) })),
        el("div", { class: "card-body" }, countries.length ? barList(countries, { max: Math.max(1, ...countries.map((c) => c.value)), format: (v) => fmtNum(v) + (totalActive ? " · " + fmtPct(v * 100 / totalActive, 0) : "") }) : empty("globe", t("Nobody yet"), null)))),
    card(cardHead(t("Who comes back, by week of sign-up"), t("Back on day N: active exactly N days after the account was made")),
      cohorts.length ? table([t("Accounts made"), { label: t("Accounts"), cls: "num" }, t("Day 1"), t("Day 7"), t("Day 30")], cohorts, { cls: "heat" }) : empty("users", t("No accounts yet"), null)),
    currencies.length ? el("div", {}, el("div", { class: "section-head" }, el("h2", {}, t("Revenue by day")), el("span", { class: "sub" }, t("The last 30 days · test payments left out"))), el("div", { class: "grid-2" }, revenueCharts)) : null,
    card(cardHead(t("“Play now”: the latest waits"), t("Each pairing counts both players")), el("div", { class: "card-body" }, waits)));
};

// ================================================================= audit log

PAGES.audit = async (route) => {
  const q = route.query.get("q") || "";
  const action = route.query.get("action") || "";
  const target = route.query.get("target") || "";
  const params = (before) => "/audit?" + new URLSearchParams({ q, action, target, ...(before ? { before } : {}) });
  const first = await api(params());
  let entries = first.entries.slice(), next = first.next;
  const search = el("input", { class: "input wide", type: "search", placeholder: t("Staff, player or words"), value: q, "aria-label": t("Search"), "data-focus": "audit-q" });
  search.addEventListener("input", debounce(() => { setQuery({ q: search.value.trim() }); render({ quiet: true }); }, 350));
  const actionSel = el("select", { class: "select", "aria-label": t("Action"), "data-focus": "audit-action" }, el("option", { value: "" }, t("Every action")),
    Object.keys(AUDIT_ACTIONS).sort((a, b) => auditLabel(a).localeCompare(auditLabel(b), lang)).map((a) => el("option", { value: a }, auditLabel(a))));
  actionSel.value = action;
  actionSel.addEventListener("change", () => { setQuery({ action: actionSel.value }); render(); });
  const tbody = el("tbody");
  const more = el("div", { class: "pager" });
  const row = (e) => {
    const [tone, ic] = AUDIT_TONE[e.action] || ["", "info"];
    return el("tr", {},
      el("td", { class: "nowrap" }, fmtDate(e.at)),
      el("td", {}, personLink(e.staff)),
      el("td", {}, badge(auditLabel(e.action), tone, ic)),
      el("td", {}, PLAYER_ACTIONS.has(e.action) && e.target ? personLink(e.target, { avatar: false }) : el("span", { class: /^#?\d+$/.test(e.target || "") ? "mono" : "" }, e.action === "login" ? "—" : auditTarget(e))),
      // Moderators get no address for a sign-in: nothing to show then.
      el("td", { class: "wrap muted" }, e.action === "login" ? (e.details ? tf("from {ip}", { ip: e.details }) : null) : e.details));
  };
  const drawMore = () => {
    setKids(more, el("span", {}, tn(entries.length, "{n} entry shown", "{n} entries shown")),
      el("div", { class: "row" },
        btn(t("Export CSV"), { kind: "sm", icon: "download", disabled: !entries.length, onClick: () => exportCsv(entries) }),
        next ? btn(t("Load more"), { kind: "sm primary", onClick: async (ev) => {
          ev.currentTarget.disabled = true;
          try {
            const d = await api(params(next));
            entries = entries.concat(d.entries);
            next = d.next;
            for (const e of d.entries) tbody.append(row(e));
          } catch (err) { toast(err.message, "error"); }
          drawMore();
        } }) : null));
  };
  for (const e of entries) tbody.append(row(e));
  drawMore();
  return el("div", {},
    pageHead(t("Audit log"), t("Every staff action, newest first")),
    el("div", { class: "toolbar" }, search, actionSel,
      target ? el("span", { class: "badge accent" }, tf("Player: {name}", { name: target }), btn(t("Clear"), { kind: "sm ghost icon-only", icon: "x", iconOnly: true, onClick: () => { setQuery({ target: null }); render(); } })) : null),
    card(null, entries.length
      ? el("div", { class: "table-wrap" }, el("table", { class: "data" }, el("thead", {}, el("tr", {}, [t("When"), t("Staff"), t("Action"), t("Target"), t("Details")].map((h) => el("th", {}, h)))), tbody))
      : empty("audit", t("Nothing found"), q || action ? t("Try other words or another action.") : t("Every staff action is written here.")),
    entries.length ? more : null));
};
function exportCsv(entries) {
  const cell = (v) => {
    let s = String(v ?? "");
    if (/^[=+\-@\t\r]/.test(s)) s = "'" + s; // never a formula in a spreadsheet
    return '"' + s.replaceAll('"', '""') + '"';
  };
  const lines = [["id", "when", "staff", "action", "target", "details"].join(",")]
    .concat(entries.map((e) => [e.id, new Date(e.at * 1000).toISOString(), e.staff, e.action, e.target, e.details].map(cell).join(",")));
  const blob = new Blob(["﻿" + lines.join("\r\n")], { type: "text/csv;charset=utf-8" });
  const a = el("a", { href: URL.createObjectURL(blob), download: "snook-legend-audit-" + new Date().toISOString().slice(0, 10) + ".csv" });
  document.body.append(a);
  a.click();
  a.remove();
}

// ==================================================================== server

const WEEKDAYS = (d) => new Intl.DateTimeFormat(lang, { weekday: "long", timeZone: "UTC" }).format(new Date(Date.UTC(2023, 0, 1 + d)));
PAGES.system = async () => {
  const [s, staffList] = await Promise.all([api("/system"), api("/users?filter=staff&sort=name")]);
  every(30000, () => refreshIf("system"));
  const r = s.settings;
  const kv = (rows) => el("dl", { class: "kv" }, rows.filter(Boolean).map(([k, v]) => [el("dt", {}, k), el("dd", {}, v)]));
  const cup = r.cup ? kv([
    [t("Name"), r.cup.name],
    [t("When"), tf("{day} at {time} (UTC{offset})", { day: WEEKDAYS(r.cup.weekday), time: String(r.cup.hour).padStart(2, "0") + ":" + String(r.cup.minute).padStart(2, "0"), offset: (r.cup.utc_offset >= 0 ? "+" : "−") + Math.abs(r.cup.utc_offset / 60) })],
    [t("Lasts"), fmtMinutes(r.cup.minutes)],
    [t("Entry"), r.cup.entry_fee ? tf("{n} coins", { n: fmtNum(r.cup.entry_fee) }) : t("free")],
    [t("Rules"), rulesBadge(r.cup.rules, true)],
  ]) : el("p", { class: "faint" }, t("No weekly cup is set up (SNOOK_LEGEND_CUP)."));
  return el("div", { class: "stack" },
    pageHead(t("Server"), t("What runs, how it is set up, how busy it is"), updatedLabel(refreshPage)),
    el("div", { class: "kpis" },
      kpi({ label: t("Version"), value: s.version, sub: tf("protocol {p}", { p: s.protocol }), iconName: "server" }),
      kpi({ label: t("Up for"), value: fmtDuration(s.uptime_secs), sub: tf("since {when}", { when: fmtDate(s.now - s.uptime_secs) }), iconName: "clock", tone: "success" }),
      kpi({ label: t("Memory"), value: fmtBytes(s.memory_bytes), iconName: "memory", tone: "info" }),
      kpi({ label: t("Connections"), value: fmtNum(s.online), sub: tn(s.addresses, "{n} address", "{n} addresses") + " · " + perRulesText(s.online_by_rules), iconName: "plug" }),
      kpi({ label: t("Games now"), value: fmtNum(s.games), sub: tn(s.tables, "{n} open table", "{n} open tables"), iconName: "activity", tone: "warning" }),
      kpi({ label: t("Writes waiting"), value: fmtNum(s.pending_writes), sub: t("results, saves and payments on their way to the database"), iconName: "queue", tone: s.pending_writes > 50 ? "danger" : "" })),
    el("div", { class: "grid-2" },
      card(cardHead(t("Health")), el("div", { class: "card-body health" },
        healthRow(s.persistent, s.persistent ? t("Accounts and games are saved in PostgreSQL") : t("Everything is kept in memory: nothing is saved")),
        healthRow(!s.draining, s.draining ? t("Restarting: no new games") : t("Taking new games")),
        healthRow(!(s.online >= 4 && s.addresses === 1), s.online >= 4 && s.addresses === 1 ? t("Every player comes from one address: set SNOOK_LEGEND_CLIENT_IP_HEADER") : t("Players' addresses are told apart"), null, true),
        healthRow(s.stripe, s.stripe ? t("Card payments (Stripe) are on") : t("Card payments (Stripe) are off"), null, true),
        healthRow(s.msstore, s.msstore ? t("Microsoft Store purchases are on") : t("Microsoft Store purchases are off"), null, true),
        healthRow(s.pending_writes <= 50, tn(s.pending_writes, "{n} write waiting", "{n} writes waiting"), null, true),
        healthRow(true, tf("Only version {p} of the game is let in", { p: s.oldest_protocol === s.protocol ? s.protocol : s.oldest_protocol + "–" + s.protocol })))),
      card(cardHead(t("Server settings")), el("div", { class: "card-body" }, kv([
        [t("Shot clock"), tf("{s} s", { s: r.shot_clock_secs })],
        [t("Time to come back after a drop"), tf("{s} s", { s: r.reconnect_grace_secs })],
        [t("Lounge chat kept"), tn(r.chat_history, "{n} line", "{n} lines")],
        [t("Connections per address"), fmtNum(r.max_connections_per_ip)],
        [t("Tournament no-show"), tf("{s} s", { s: r.tournament_no_show_secs })],
        [t("Open tournaments at most"), fmtNum(r.max_open_tournaments)],
        [t("“Looking for a game” notes last"), fmtDuration(r.seek_lifetime_secs)],
        [t("Local network server"), r.lan ? t("Yes") : t("No")],
      ])))),
    el("div", { class: "grid-2" },
      card(cardHead(t("Weekly cup")), el("div", { class: "card-body" }, cup)),
      card(cardHead(t("Staff"), tn(staffList.total, "{n} member", "{n} members")), staffList.users.length ? table([t("Name"), t("Role"), t("Last seen")], staffList.users.map((u) => clickable(el("tr", { class: "click" },
        el("td", {}, personLink(u.username)), el("td", {}, badge(roleLabel(u.role), u.role === "admin" ? "danger" : "info", "shield")), el("td", {}, u.online ? badge(t("Online"), "success dot") : timeEl(u.last_seen))), () => go("players/" + encodeURIComponent(u.username)))))
        : empty("shield", t("No staff yet"), null))));
};

// ============================================================ replay viewer

/** A stored game as a file (the strokes). */
async function downloadReplay(id) {
  try {
    const r = await api("/replay/" + id);
    const blob = new Blob([JSON.stringify(r, null, 2)], { type: "application/json" });
    const a = el("a", { href: URL.createObjectURL(blob), download: "snook-legend-game-" + id + ".json" });
    document.body.append(a);
    a.click();
    a.remove();
  } catch (e) { toast(e.message, "error"); }
}

let viewer = null;
const BALL_COLORS = ["#f4efe2", "#f2c200", "#2051c9", "#d42a2a", "#6c2fa3", "#f07a1a", "#1d8a3a", "#8a2323", "#141414"];
const ballColor = (n) => BALL_COLORS[n > 8 ? n - 8 : n];
const POCKETS = { "head left corner": "the head-left corner", "head right corner": "the head-right corner", "left side": "the left side pocket", "right side": "the right side pocket", "foot left corner": "the foot-left corner", "foot right corner": "the foot-right corner" };
const pocketName = (p) => t(POCKETS[String(p)] || String(p));
/** A foul as the server describes it, in the chosen language. */
function foulText(f) {
  let m = String(f).match(/^Wrong ball hit first \((.+)\)$/);
  if (m) return tf("Wrong ball hit first ({id})", { id: m[1] });
  m = String(f).match(/^The (.+) left the table$/);
  if (m) return tf("The {id} left the table", { id: m[1] });
  m = String(f).match(/^The (.+) behind the head string was hit without crossing it first$/);
  if (m) return tf("The {id} behind the head string was hit without crossing it first", { id: m[1] });
  return t(f);
}

function closeViewer() {
  if (!viewer) return;
  cancelAnimationFrame(viewer.raf);
  clearTimeout(viewer.hold);
  viewer.box.remove();
  if (viewer.opener && document.contains(viewer.opener)) viewer.opener.focus();
  viewer = null;
}

/** Opens a game in the replay viewer; `fallback` is tried when `path`
 * has no such game (a number typed in the search may be a live room). */
async function openViewer(path, fallback) {
  closeViewer();
  const card = el("div", { class: "viewer-card card" }, el("div", { class: "viewer-wait" }, el("span", { class: "spinner" }), t("Rebuilding the game…")));
  // tabindex -1: the keyboard moves into the viewer while it loads.
  const box = el("div", { class: "viewer", role: "dialog", "aria-modal": "true", "aria-label": t("Replay"), tabindex: "-1" }, card);
  box.addEventListener("mousedown", (e) => { if (e.target === box) closeViewer(); });
  document.body.append(box);
  viewer = { box, raf: 0, opener: document.activeElement };
  box.focus({ preventScroll: true });
  let data;
  try {
    try {
      data = await api(path);
    } catch (e) {
      if (e.status !== 404 || !fallback) throw e;
      try { data = await api(fallback); } catch (_) { throw e; }
    }
  } catch (e) {
    if (viewer && viewer.box === box) {
      const close = btn(t("Close"), { onClick: closeViewer });
      setKids(card, alertBox("danger", t("The game couldn't be opened"), e.message), el("div", { class: "row" }, close));
      close.focus();
    }
    return;
  }
  if (!viewer || viewer.box !== box) return;
  buildViewer(card, data);
}

/** A player's answer after some breaks of the official rules. */
const CHOICES = {
  play_on: "chose to play on", cue_ball_in_hand: "chose the cue ball in hand behind the head string",
  break_again: "chose to break again", opponent_breaks: "chose to have the other player break",
};
/** What a break left to decide (the official rules, 4.3). */
function breakChoiceText(c) {
  const k = typeof c === "string" ? c : Object.keys(c || {})[0];
  return {
    illegal_break: t("Illegal break: the other player chooses how the game goes on"),
    eight_on_break: t("The 8 on the break: it is spotted, and the breaker chooses how the game goes on"),
    eight_on_foul_break: t("The 8 on a foul break: it is spotted, and the other player chooses how the game goes on"),
    ball_off_on_break: t("A ball off the table on the break: the other player chooses how the game goes on"),
  }[k] || k;
}
const isFoulStroke = (s) => s.kind === "resign" || s.kind === "abandon" || s.kind === "time_foul" || (s.kind === "shot" && s.fouls.length > 0);

function strokeSummary(s, names) {
  const who = names[s.seat] || tf("Player {n}", { n: s.seat + 1 });
  if (s.kind === "resign") return [who, t("resigned")];
  if (s.kind === "abandon") return [who, t("left the game")];
  if (s.kind === "time_foul") return [who, t("ran out of time")];
  if (s.kind === "choice") return [who, t(CHOICES[s.option] || String(s.option))];
  const made = s.pocketed.filter((p) => p.ball !== 0).map((p) => p.ball);
  const parts = [];
  if (s.break) parts.push(t("break"));
  parts.push(made.length ? tf("made {balls}", { balls: made.join(", ") }) : s.first === null ? t("hit nothing") : t("made nothing"));
  if (s.fouls.length) parts.push(t("foul"));
  return [who, parts.join(" · ")];
}
function strokeDetails(s, rules) {
  if (s.kind !== "shot") return [];
  const out = [];
  // The official rules call every shot but the break (no call: a safety);
  // the Brazilian rules call only the 8.
  if (rules === "official" && !s.break) out.push(s.called ? tf("Called {pocket}", { pocket: pocketName(s.called) }) : t("No call: a safety"));
  else if (s.called) out.push(tf("8-ball called in {pocket}", { pocket: pocketName(s.called) }));
  for (const p of s.pocketed) out.push(p.ball === 0 ? tf("The cue ball in {pocket}", { pocket: pocketName(p.pocket) }) : tf("The {n} in {pocket}", { n: p.ball, pocket: pocketName(p.pocket) }));
  for (const f of s.fouls) out.push(tf("Foul: {foul}", { foul: foulText(f) }));
  if (s.penalty && s.penalty.length) out.push(tf("Penalty: {balls} off the table", { balls: s.penalty.join(", ") }));
  if (s.break_again) out.push(t("The break is played again"));
  if (s.choice) out.push(breakChoiceText(s.choice));
  if (s.in_hand) out.push(t("ball in hand"));
  if (s.timeout) out.push(t("played by the shot clock"));
  out.push(tf("power {n}%", { n: s.power }));
  return out;
}

function buildViewer(card, data) {
  const tb = data.table, m = data.match, names = m.players;
  const R = tb.ball_radius;
  const rail = 120, L = tb.length, Wd = tb.width;
  const vb = [-L / 2 - rail, -Wd / 2 - rail, L + 2 * rail, Wd + 2 * rail];
  const board = svg("svg", { viewBox: vb.join(" "), class: "table-svg", role: "img", "aria-label": t("The table from above") });
  const defs = svg("defs");
  board.append(defs,
    svg("rect", { x: vb[0], y: vb[1], width: vb[2], height: vb[3], rx: 70, class: "t-rail" }),
    svg("rect", { x: -L / 2 - 38, y: -Wd / 2 - 38, width: L + 76, height: Wd + 76, rx: 18, class: "t-cushion" }),
    svg("rect", { x: -L / 2, y: -Wd / 2, width: L, height: Wd, class: "t-cloth" }),
    svg("line", { x1: -L / 4, x2: -L / 4, y1: -Wd / 2, y2: Wd / 2, class: "t-string" }),
    svg("circle", { cx: L / 4, cy: 0, r: 7, class: "t-spot" }));
  for (let k = 1; k < 8; k++) {
    if (k === 4) continue;
    for (const sy of [-1, 1]) board.append(svg("circle", { cx: -L / 2 + k * L / 8, cy: sy * (Wd / 2 + rail * 0.62), r: 7, class: "t-diamond" }));
  }
  for (let k = 1; k < 4; k++) for (const sx of [-1, 1]) board.append(svg("circle", { cx: sx * (L / 2 + rail * 0.62), cy: -Wd / 2 + k * Wd / 4, r: 7, class: "t-diamond" }));
  for (const c of tb.cushions) board.append(svg("line", { x1: c[0], y1: -c[1], x2: c[2], y2: -c[3], class: "t-jaw" }));
  for (const p of tb.pockets) board.append(svg("circle", { cx: p.x, cy: -p.y, r: p.r, class: "t-pocket" }, svg("title", {}, pocketName(p.name))));
  const balls = [];
  for (let n = 0; n < 16; n++) {
    const g = svg("g", { class: "ball" });
    const color = ballColor(n);
    if (n > 8) {
      defs.append(svg("clipPath", { id: "vb" + n }, svg("circle", { r: R })));
      g.append(svg("circle", { r: R, fill: "#f4efe2" }), svg("rect", { x: -R, y: -R * 0.55, width: 2 * R, height: R * 1.1, fill: color, "clip-path": "url(#vb" + n + ")" }));
    } else {
      g.append(svg("circle", { r: R, fill: color }));
    }
    g.append(svg("circle", { r: R, class: "ball-edge" }));
    if (n > 0) g.append(svg("circle", { r: R * 0.5, class: "ball-disc" }), svg("text", { y: R * 0.22, "text-anchor": "middle", class: "ball-num" }, n));
    g.append(svg("title", {}, n === 0 ? t("Cue ball") : tf("The {n}", { n })));
    balls.push(g);
    board.append(g);
  }
  const strokes = data.strokes;
  const st = { k: 0, f: 0, pos: [], playing: false, speed: 1, acc: 0, last: 0 };
  const startOf = (k) => (k < strokes.length ? strokes[k].start : data.end).map((p) => (p ? p.slice() : null));
  const apply = (frame) => { for (let i = 0; i + 2 < frame.length; i += 3) st.pos[frame[i]] = frame[i + 1] === null ? null : [frame[i + 1], frame[i + 2]]; };
  const frames = (k) => (k < strokes.length ? strokes[k].frames : []);
  const over = () => {
    if (data.over) return tf("{name} won ({how})", { name: names[data.over.winner] || "?", how: gameEnd(data.over.reason) });
    return m.live ? t("still being played") : t("unfinished");
  };
  const scrub = el("input", { type: "range", min: "0", max: "0", value: "0", class: "scrub", "aria-label": t("Position in the stroke") });
  const status = el("div", { class: "viewer-status" });
  const prev = btn(t("Previous stroke") + " (←)", { kind: "icon-only", icon: "chevronLeft", iconOnly: true });
  const back = el("button", { type: "button", class: "btn", title: t("One frame back") + " (,)", "aria-label": t("One frame back") }, "‹");
  const play = el("button", { type: "button", class: "btn primary play", title: t("Play or pause") + " (" + t("Space") + ")" });
  const fwd = el("button", { type: "button", class: "btn", title: t("One frame forward") + " (.)", "aria-label": t("One frame forward") }, "›");
  const next = btn(t("Next stroke") + " (→)", { kind: "icon-only", icon: "chevronRight", iconOnly: true });
  const speed = el("select", { class: "select", "aria-label": t("Speed") }, [["0.25", "¼×"], ["0.5", "½×"], ["1", "1×"], ["2", "2×"], ["4", "4×"]].map(([v, l]) => el("option", { value: v }, l)));
  speed.value = "1";
  speed.addEventListener("change", () => { st.speed = Number(speed.value); });
  const list = el("ol", { class: "stroke-list" }, strokes.map((s, k) => {
    const [who, what] = strokeSummary(s, names);
    return clickable(el("li", { class: isFoulStroke(s) ? "foul" : "" }, el("span", { class: "n" }, k + 1), el("span", { class: "who seat" + s.seat }, who), el("span", { class: "what" }, what)),
      () => { pause(); seek(k, 0); });
  }));
  const draw = () => {
    st.pos.forEach((p, n) => {
      if (p) { balls[n].setAttribute("transform", "translate(" + p[0] + "," + -p[1] + ")"); balls[n].removeAttribute("visibility"); }
      else balls[n].setAttribute("visibility", "hidden");
    });
    const fr = frames(st.k);
    scrub.max = String(fr.length);
    scrub.value = String(st.f);
    scrub.disabled = fr.length === 0;
    setKids(play, icon(st.playing ? "pause" : "play"), st.playing ? t("Pause") : t("Play"));
    prev.disabled = st.k === 0 && st.f === 0;
    next.disabled = st.k >= strokes.length;
    const s = strokes[st.k];
    if (s) {
      const [who, what] = strokeSummary(s, names);
      setKids(status, el("b", {}, tf("Stroke {n} of {total}", { n: st.k + 1, total: strokes.length })), " · ", who, " · ", what, el("div", { class: "muted details" }, strokeDetails(s, data.rules).join(" · ")));
    } else {
      setKids(status, el("b", {}, t("End of the game")), " · ", over());
    }
    list.querySelectorAll("li").forEach((li, k) => li.classList.toggle("current", k === st.k));
    const cur = list.children[Math.min(st.k, list.children.length - 1)];
    if (cur && st.playing) cur.scrollIntoView({ block: "nearest" });
  };
  const seek = (k, f) => {
    st.k = Math.max(0, Math.min(k, strokes.length));
    st.pos = startOf(st.k);
    const fr = frames(st.k);
    st.f = Math.max(0, Math.min(f, fr.length));
    for (let i = 0; i < st.f; i++) apply(fr[i]);
    draw();
  };
  const pause = () => { st.playing = false; clearTimeout(viewer.hold); draw(); };
  const tick = (now) => {
    if (!viewer || !st.playing) return;
    const dt = st.last ? Math.min(0.1, (now - st.last) / 1000) : 0;
    st.last = now;
    st.acc += dt * data.fps * st.speed;
    const fr = frames(st.k);
    let moved = false;
    while (st.acc >= 1 && st.f < fr.length) { apply(fr[st.f]); st.f++; st.acc -= 1; moved = true; }
    if (moved) draw();
    if (st.f >= fr.length) {
      st.acc = 0;
      if (st.k >= strokes.length) { pause(); return; }
      clearTimeout(viewer.hold);
      viewer.hold = setTimeout(() => {
        if (!viewer || !st.playing) return;
        seek(st.k + 1, 0);
        st.last = 0;
        viewer.raf = requestAnimationFrame(tick);
      }, 650 / st.speed);
      return;
    }
    viewer.raf = requestAnimationFrame(tick);
  };
  const startPlay = () => {
    if (st.k >= strokes.length) seek(0, 0);
    st.playing = true; st.last = 0; st.acc = 0;
    draw();
    cancelAnimationFrame(viewer.raf);
    viewer.raf = requestAnimationFrame(tick);
  };
  const toggle = () => (st.playing ? pause() : startPlay());
  const stepFrame = (d) => {
    pause();
    const fr = frames(st.k);
    if (d > 0) { if (st.f < fr.length) seek(st.k, st.f + 1); else if (st.k < strokes.length) seek(st.k + 1, 0); }
    else if (st.f > 0) seek(st.k, st.f - 1);
    else if (st.k > 0) seek(st.k - 1, frames(st.k - 1).length);
  };
  const stepStroke = (d) => { pause(); if (d < 0 && st.f > 0) seek(st.k, 0); else seek(st.k + d, 0); };
  prev.addEventListener("click", () => stepStroke(-1));
  next.addEventListener("click", () => stepStroke(1));
  back.addEventListener("click", () => stepFrame(-1));
  fwd.addEventListener("click", () => stepFrame(1));
  play.addEventListener("click", toggle);
  scrub.addEventListener("input", () => { pause(); seek(st.k, Number(scrub.value)); });
  const close = btn(t("Close") + " (Esc)", { kind: "ghost icon-only", icon: "x", iconOnly: true, onClick: closeViewer });
  setKids(card, 
    el("div", { class: "viewer-head" },
      el("div", { class: "viewer-title" }, el("b", {}, names[0] + " × " + names[1]),
        el("span", { class: "muted row" }, rulesBadge(data.rules, true), [m.live ? t("in progress") : m.rated ? t("rated") : t("casual"), m.live ? t("so far") : fmtDate(m.at), m.id ? "#" + m.id : null].filter(Boolean).join(" · "))),
      el("div", { class: "row" }, m.id && !m.live ? btn(t("Download"), { kind: "sm ghost", icon: "download", onClick: () => downloadReplay(m.id) }) : null, close)),
    el("div", { class: "viewer-body" },
      el("div", { class: "viewer-stage" }, board, scrub, el("div", { class: "viewer-controls" }, prev, back, play, fwd, next, speed), status),
      el("div", { class: "viewer-side" }, el("div", { class: "side-head" }, tn(strokes.length, "{n} stroke", "{n} strokes") + " · " + over()), list)),
    data.error ? alertBox("warning", t("The replay stops early"), data.error) : null);
  viewer.keys = (e) => {
    // Already handled (Enter or Space on a stroke of the list).
    if (e.defaultPrevented) return;
    if (e.target && ["INPUT", "SELECT", "TEXTAREA"].includes(e.target.tagName) && e.target !== scrub) return;
    if (e.key === " ") { e.preventDefault(); toggle(); }
    else if (e.key === "ArrowLeft") { e.preventDefault(); stepStroke(-1); }
    else if (e.key === "ArrowRight") { e.preventDefault(); stepStroke(1); }
    else if (e.key === ",") stepFrame(-1);
    else if (e.key === ".") stepFrame(1);
  };
  seek(0, 0);
  play.focus();
}

// ===================================================================== start

applyTheme();
start();
