// Snookered staff panel. Plain JavaScript, no dependencies; every value from the
// server is inserted with textContent (never as HTML).
"use strict";

const $ = (sel) => document.querySelector(sel);
let token = sessionStorage.getItem("snookered-admin") || "";
let me = null;
let view = "dashboard";

function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return e;
}

function toast(text) {
  const t = $("#toast");
  t.textContent = text;
  t.classList.remove("hidden");
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => t.classList.add("hidden"), 3000);
}

async function api(path, options = {}) {
  const res = await fetch("/admin/api" + path, {
    method: options.method || "GET",
    headers: { "Content-Type": "application/json", ...(token ? { Authorization: "Bearer " + token } : {}) },
    body: options.body ? JSON.stringify(options.body) : undefined,
  });
  let data = {};
  try { data = await res.json(); } catch (_) { /* empty */ }
  if (res.status === 401) { signOut(false); throw new Error(data.error || "Sign in again."); }
  if (!res.ok) throw new Error(data.error || "Request failed (" + res.status + ").");
  return data;
}

const fmtDate = (s) => (s ? new Date(s * 1000).toLocaleString() : "—");
const ago = (s) => {
  if (!s) return "—";
  const d = Math.max(0, Date.now() / 1000 - s);
  if (d < 90) return "just now";
  if (d < 5400) return Math.round(d / 60) + " min ago";
  if (d < 129600) return Math.round(d / 3600) + " h ago";
  return Math.round(d / 86400) + " days ago";
};
const until = (s) => (s >= 9e15 ? "permanently" : "until " + fmtDate(s));

function statusTags(u) {
  const now = Date.now() / 1000;
  return el("span", {},
    u.online ? el("span", { class: "tag on" }, "online") : null, " ",
    u.role !== "player" ? el("span", { class: "tag staff " + u.role }, u.role) : null, " ",
    u.banned_until && u.banned_until > now ? el("span", { class: "tag ban" }, "banned") : null, " ",
    u.muted_until && u.muted_until > now ? el("span", { class: "tag mute" }, "muted") : null);
}

function table(headers, rows) {
  return el("div", { class: "table-wrap" },
    el("table", {}, el("thead", {}, el("tr", {}, headers.map((h) => el("th", {}, h)))), el("tbody", {}, rows)));
}

function setView(name) {
  view = name;
  document.querySelectorAll("nav button").forEach((b) => b.classList.toggle("active", b.dataset.view === name));
  render();
}

async function render() {
  const main = $("#main");
  try {
    const content = await views[view]();
    main.replaceChildren(content);
  } catch (e) {
    main.replaceChildren(el("p", { class: "error" }, e.message));
  }
}

const views = {
  async dashboard() {
    const o = await api("/overview");
    $("#report-count").textContent = o.stored.open_reports || "";
    const stat = (n, l) => el("div", { class: "card stat" }, el("div", { class: "n" }, n), el("div", { class: "l" }, l));
    const h = Math.floor(o.uptime_secs / 3600), m = Math.floor((o.uptime_secs % 3600) / 60);
    const box = el("div", {},
      el("div", { class: "grid" },
        stat(o.online, "Online now"), stat(o.matches_live, "Games now"), stat(o.tables_open, "Open tables"),
        stat(o.tournaments, "Tournaments"), stat(o.stored.users, "Accounts"), stat(o.stored.new_users_7d, "New (7 days)"),
        stat(o.stored.active_users_7d, "Active (7 days)"), stat(o.stored.matches_24h, "Games (24 h)"),
        stat(o.stored.matches_total, "Games (all)"), stat(o.stored.open_reports, "Open reports"),
        stat(o.stored.banned, "Banned"), stat(h + "h " + m + "m", "Uptime · v" + o.version)),
      o.persistent ? null : el("p", { class: "error" }, "This server keeps everything in memory: nothing is saved."),
      o.online >= 4 && o.addresses === 1 ? el("p", { class: "error" }, "Every player comes from the same address: set SNOOKERED_CLIENT_IP_HEADER (see DEPLOY), or the per-address limits will block players.") : null);
    const announce = el("textarea", { rows: "2", maxlength: "200", placeholder: "Message to everyone in the lounge" });
    box.append(el("h2", {}, "Announcement"), announce,
      el("div", { class: "actions" }, el("button", {
        class: "primary", onclick: async () => {
          try { await api("/announce", { method: "POST", body: { text: announce.value } }); announce.value = ""; toast("Sent."); }
          catch (e) { toast(e.message); }
        },
      }, "Send to the lounge")));
    // The lounge's board: notes that stay up (rules, events, tournaments).
    const notes = el("div", {});
    const loadNotes = async () => {
      const d = await api("/board");
      const rows = d.notes.map((n) => el("tr", {},
        el("td", {}, n.pinned ? "📌 " : "", n.text, n.action ? el("span", { class: "muted" }, " · button: " + n.action) : null),
        el("td", {}, n.kind), el("td", {}, n.author || "—"), el("td", {}, ago(n.created_at)),
        el("td", {}, n.expires_at ? fmtDate(n.expires_at) : "—"),
        el("td", {}, el("button", {
          class: "danger", onclick: async () => {
            try { await api("/board/" + n.id, { method: "DELETE" }); toast("Taken down."); loadNotes(); }
            catch (e) { toast(e.message); }
          },
        }, "Take down"))));
      notes.replaceChildren(rows.length
        ? table(["Note", "Kind", "By", "Pinned", "Until", ""], rows)
        : el("p", { class: "muted" }, "The board is empty."));
    };
    const note = el("textarea", { rows: "3", maxlength: "400", placeholder: "A note for the lounge's board" });
    const days = el("input", { type: "number", min: "1", max: "366", placeholder: "Days (empty: until taken down)" });
    const kind = el("select", {}, [["notice", "Notice (gold)"], ["event", "Event (green)"], ["rule", "Rule (blue)"]]
      .map(([v, l]) => el("option", { value: v }, l)));
    const action = el("select", {}, [["", "No button"], ["tournaments", "Button: tournaments"], ["shop", "Button: shop"],
      ["vip", "Button: VIP"], ["missions", "Button: missions"]].map(([v, l]) => el("option", { value: v }, l)));
    const pinned = el("input", { type: "checkbox", class: "check" });
    box.append(el("h2", {}, "Board"), note, days,
      el("div", { class: "actions" }, kind, action, el("label", {}, pinned, " Pinned at the top")),
      el("div", { class: "actions" }, el("button", {
        class: "primary", onclick: async () => {
          try {
            await api("/board", { method: "POST", body: {
              text: note.value, days: days.value ? Number(days.value) : null,
              kind: kind.value, action: action.value || null, pinned: pinned.checked,
            } });
            note.value = ""; days.value = ""; pinned.checked = false; action.value = ""; toast("Pinned."); loadNotes();
          } catch (e) { toast(e.message); }
        },
      }, "Pin to the board")), notes);
    loadNotes().catch((e) => toast(e.message));
    if (me && me.role === "admin") {
      box.append(el("h2", {}, "Lounge chat"), el("div", { class: "actions" }, el("button", {
        class: "danger", onclick: async () => {
          if (!confirm("Clear the lounge chat for everyone?")) return;
          try { await api("/chat/clear", { method: "POST" }); toast("The chat was cleared."); }
          catch (e) { toast(e.message); }
        },
      }, "Clear the chat")));
      const motd = el("input", { maxlength: "300" });
      motd.value = o.motd;
      box.append(el("h2", {}, "Welcome message"), motd, el("div", { class: "actions" }, el("button", {
        onclick: async () => {
          try { await api("/motd", { method: "POST", body: { text: motd.value } }); toast("Saved (until the server restarts)."); }
          catch (e) { toast(e.message); }
        },
      }, "Save")));
    }
    return box;
  },

  async users() {
    const box = el("div", {});
    const search = el("input", { placeholder: "Search by username", value: views.users.q || "" });
    const results = el("div", {});
    const load = async (page = 0) => {
      views.users.q = search.value;
      const d = await api("/users?q=" + encodeURIComponent(search.value) + "&page=" + page);
      const rows = d.users.map((u) => el("tr", { class: "click", onclick: () => showUser(u.username) },
        el("td", {}, u.username), el("td", {}, u.country || "—"), el("td", {}, u.rating),
        el("td", {}, u.games), el("td", {}, ago(u.last_seen)), el("td", {}, statusTags(u))));
      const pages = Math.ceil(d.total / d.per_page);
      results.replaceChildren(
        el("p", { class: "muted" }, d.total + " players"),
        table(["Player", "Country", "Rating", "Games", "Last seen", ""], rows),
        el("div", { class: "actions" },
          page > 0 ? el("button", { onclick: () => load(page - 1) }, "Previous") : null,
          page + 1 < pages ? el("button", { onclick: () => load(page + 1) }, "Next") : null));
    };
    search.addEventListener("keydown", (e) => { if (e.key === "Enter") load(); });
    box.append(el("div", { class: "row" }, search, el("button", { onclick: () => load() }, "Search")), results);
    await load();
    return box;
  },

  async reports() {
    const filter = views.reports.filter || "open";
    const d = await api("/reports?status=" + filter);
    const select = el("select", { onchange: () => { views.reports.filter = select.value; render(); } },
      ["open", "actioned", "dismissed", "all"].map((s) => {
        const o = el("option", { value: s }, s);
        if (s === filter) o.selected = true;
        return o;
      }));
    const list = d.reports.map((r) => el("div", { class: "card report" },
      el("div", { class: "head" },
        el("b", {}, "#" + r.id + " · " + r.reason),
        el("span", { class: "muted" }, fmtDate(r.created_at))),
      el("p", {}, el("a", { href: "#", onclick: (e) => { e.preventDefault(); showUser(r.target); } }, r.target),
        " reported by ", el("a", { href: "#", onclick: (e) => { e.preventDefault(); showUser(r.reporter); } }, r.reporter)),
      r.details ? el("p", {}, r.details) : null,
      gameBlock(r),
      r.context ? el("pre", {}, r.context) : null,
      r.status === 0
        ? el("div", { class: "actions" },
          el("button", { class: "primary", onclick: () => resolve(r.id, "actioned") }, "Action taken"),
          el("button", { onclick: () => resolve(r.id, "dismissed") }, "Dismiss"))
        : el("p", { class: "muted" }, (r.status === 1 ? "Actioned" : "Dismissed") + " by " + (r.resolved_by || "?") + (r.resolution ? ": " + r.resolution : ""))));
    return el("div", {}, el("div", { class: "row" }, el("span", {}, "Show"), select),
      list.length ? list : el("p", { class: "muted" }, "Nothing here."));
  },

  async live() {
    const d = await api("/live");
    const rooms = d.rooms.map((r) => el("tr", {},
      el("td", {}, "#" + r.id), el("td", {}, r.players[0] + " vs " + r.players[1]),
      el("td", {}, r.rated ? "rated" : "casual"), el("td", {}, r.tournament ? "yes" : ""),
      el("td", {}, r.shots), el("td", {}, r.finished ? "finished" : "playing"),
      el("td", {}, el("button", { onclick: () => openViewer("/live/" + r.id + "/view") }, "Watch"))));
    const players = d.players.map((p) => el("tr", { class: "click", onclick: () => showUser(p.username) },
      el("td", {}, p.username), el("td", {}, p.rating), el("td", {}, p.status), el("td", {}, p.country || "—")));
    const tournaments = d.tournaments.map((t) => el("tr", {},
      el("td", {}, t.name), el("td", {}, t.host), el("td", {}, t.players.length + " / " + t.size),
      el("td", {}, t.state), el("td", {}, t.champion || "")));
    return el("div", {},
      el("div", { class: "actions" }, el("button", { onclick: render }, "Refresh")),
      el("h2", {}, "Games (" + d.rooms.length + ")"), table(["", "Players", "", "Tournament", "Shots", "", ""], rooms),
      el("h2", {}, "Tournaments (" + d.tournaments.length + ")"), table(["Name", "Host", "Players", "State", "Champion"], tournaments),
      el("h2", {}, "Online (" + d.players.length + ")"), table(["Player", "Rating", "Status", "Country"], players));
  },

  async metrics() {
    const d = await api("/metrics");
    const m = d.metrics;
    const stat = (n, l, sub) => el("div", { class: "card stat" }, el("div", { class: "n" }, n), el("div", { class: "l" }, l),
      sub ? el("div", { class: "sub muted" }, sub) : null);
    const pct = (x) => (Math.round(x * 10) / 10) + "%";
    const f = m.funnel;
    const funnel = (n) => f[0] ? Math.round(n * 100 / f[0]) + "%" : "—";
    const q = d.queue;
    const waitingRated = q.waiting.filter((w) => w.rated).length;
    const longest = q.waiting.reduce((a, w) => Math.max(a, w.secs), 0);
    const hours = m.hours;
    const hourLabel = (h) => {
      const t = new Date(h * 3600 * 1000);
      return String(t.getHours()).padStart(2, "0") + "h";
    };
    const hourTitle = (h) => new Date(h * 3600 * 1000).toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" });
    const sampled = hours.some((h) => h.peak_players > 0 || h.active > 0);
    const playersChart = sampled
      ? chart({
        labels: hours.map((h) => hourLabel(h.hour)), titles: hours.map((h) => hourTitle(h.hour)),
        bars: { values: hours.map((h) => h.active), name: "Active players", cls: "c-gold" },
        line: { values: hours.map((h) => h.peak_players), name: "Peak online at once", cls: "c-blue" },
        every: 6,
      })
      : empty("No samples yet: the server counts who is online every minute, so the first hour shows up shortly.");
    const gamesChart = hours.some((h) => h.games > 0 || h.peak_games > 0)
      ? chart({
        labels: hours.map((h) => hourLabel(h.hour)), titles: hours.map((h) => hourTitle(h.hour)),
        bars: { values: hours.map((h) => h.games), name: "Games finished", cls: "c-green" },
        line: { values: hours.map((h) => h.peak_games), name: "Peak games at once", cls: "c-blue" },
        every: 6,
      })
      : empty("No games in the last 48 hours.");
    // Revenue: one chart per currency (amounts in different currencies don't add up).
    const today = Math.floor(d.now / 86400);
    const days = Array.from({ length: 30 }, (_, k) => today - 29 + k);
    const dayLabel = (day) => new Date(day * 86400 * 1000).toLocaleDateString([], { day: "2-digit", month: "2-digit" });
    const currencies = [...new Set(m.revenue_days.map((r) => r[1]))].sort();
    const revenueCharts = currencies.length
      ? currencies.map((cur) => {
        const by = new Map(m.revenue_days.filter((r) => r[1] === cur).map((r) => [r[0], r]));
        const total = m.revenue.find((r) => r[0] === cur) || [cur, 0, 0];
        return el("div", { class: "card chart-card" },
          el("div", { class: "chart-head" }, el("b", {}, cur.toUpperCase()),
            el("span", { class: "muted" }, money(total[1], cur) + " · " + total[2] + (total[2] === 1 ? " payment" : " payments"))),
          chart({
            labels: days.map(dayLabel), titles: days.map((day) => new Date(day * 86400 * 1000).toLocaleDateString()),
            bars: { values: days.map((day) => (by.get(day) || [0, 0, 0, 0])[2] / 100), name: "Revenue (" + cur.toUpperCase() + ")", cls: "c-green", format: (v) => v.toFixed(2) },
            every: 6, height: 190, width: 520,
          }));
      })
      : [empty("No payments in the last 30 days (test payments are left out).")];
    const recent = q.recent || [];
    const waitsChart = recent.length
      ? chart({
        labels: recent.map(() => ""), titles: recent.map((s, k) => "Pairing " + (k + 1) + " of the latest " + recent.length),
        bars: { values: recent, name: "Wait (s)", cls: "c-gold" }, every: 0, height: 130, unit: " s",
      })
      : empty("Nobody has been paired by \"Play now\" since the server started.");
    const maxActive = Math.max(1, ...m.days.map((x) => x[1]));
    const bars = m.days.map(([day, active, games, accounts]) => el("tr", {},
      el("td", {}, fmtDate(day * 86400).split(",")[0]),
      el("td", {}, barOf(active / maxActive), " " + active),
      el("td", {}, games), el("td", {}, accounts)));
    return el("div", { class: "metrics" },
      d.draining ? el("p", { class: "error" }, "The server is restarting (draining): no new games.") : null,
      el("div", { class: "row refresh" }, el("span", { class: "muted" }, "Updated " + new Date(d.now * 1000).toLocaleTimeString() + " · refreshes every minute"),
        el("button", { onclick: render }, "Refresh")),
      el("div", { class: "grid" },
        stat(d.online, "Online now"), stat(d.games, "Games now"),
        stat(q.size, "In the queue now", q.size ? waitingRated + " rated · longest " + longest + " s" : "nobody waiting"),
        stat(q.median_wait_secs + " s", "Median wait", "average " + Math.round(q.avg_wait_secs) + " s · " + q.pairs + " pairings"),
        stat(pct(m.retention_d1), "Back the next day"), stat(pct(m.retention_d7), "Back after 7 days"),
        stat(f[0], "Accounts (30 days)"), stat(funnel(f[1]), "Played 1 game"), stat(funnel(f[2]), "Played 5 games")),
      el("h2", {}, "Players by hour (48 h)"), el("div", { class: "card chart-card" }, playersChart),
      el("h2", {}, "Games by hour (48 h)"), el("div", { class: "card chart-card" }, gamesChart),
      el("h2", {}, "Revenue by day (30 days)"), el("div", { class: "charts" }, revenueCharts),
      el("h2", {}, "\"Play now\": latest waits"), el("div", { class: "card chart-card" }, waitsChart),
      el("p", { class: "muted" }, "The funnel starts at the account: installs are not counted (the game sends nothing before signing in). Active players per hour are signed-in accounts seen online; peaks count guests too."),
      el("details", { class: "days" }, el("summary", {}, "Last 30 days, day by day"),
        table(["Day", "Active players", "Games", "New accounts"], bars)));
  },

  async aim() {
    const d = await api("/aim");
    const rows = d.flagged.map((r) => el("tr", { class: "click", onclick: () => showUser(r.username) },
      el("td", {}, r.username), el("td", {}, r.rating + " · " + r.tier),
      el("td", {}, r.made + " / " + r.attempts), el("td", {}, Math.round(r.rate * 100) + "%"),
      el("td", {}, Math.round(r.tier_mean * 100) + "% (" + r.tier_players + ")"), el("td", {}, r.z.toFixed(1))));
    return el("div", {},
      el("p", { class: "muted" }, "Pots of hard shots (cuts of 45° or more, over 1.5 m) compared with the players of the same tier. Only players with " + d.min_attempts + " or more hard shots count (" + d.players + " now). Nobody is punished by this list: open the player, watch their games (replays open from their profile in the game), and decide."),
      rows.length ? table(["Player", "Rating", "Hard pots", "Rate", "Tier average", "Deviations"], rows) : el("p", { class: "muted" }, "Nobody stands out."));
  },

  async audit() {
    const d = await api("/audit");
    return table(["When", "Staff", "Action", "Target", "Details"], d.entries.map((a) => el("tr", {},
      el("td", {}, fmtDate(a.at)), el("td", {}, a.staff), el("td", {}, a.action), el("td", {}, a.target), el("td", {}, a.details))));
  },
};

async function resolve(id, status) {
  const note = prompt("Note (optional)") || "";
  try { await api("/reports/" + id, { method: "POST", body: { status, note } }); toast("Saved."); render(); }
  catch (e) { toast(e.message); }
}

function modal(...children) {
  const m = $("#modal");
  $("#modal-body").replaceChildren(...children);
  m.classList.remove("hidden");
}
function closeModal() { $("#modal").classList.add("hidden"); }
$("#modal").addEventListener("click", (e) => { if (e.target.id === "modal") closeModal(); });

async function act(name, action, body) {
  try {
    await api("/users/" + encodeURIComponent(name) + "/" + action, { method: "POST", body: body || {} });
    toast("Done.");
    closeModal();
    showUser(name);
  } catch (e) { toast(e.message); }
}

function banDialog(name) {
  const hours = el("select", {}, [["24", "1 day"], ["72", "3 days"], ["168", "1 week"], ["720", "30 days"], ["0", "Permanent"]]
    .map(([v, l]) => el("option", { value: v }, l)));
  const reason = el("input", { placeholder: "Reason (shown to the player)", maxlength: "300" });
  modal(el("h2", {}, "Ban " + name), hours, reason, el("div", { class: "actions" },
    el("button", { class: "danger", onclick: () => act(name, "ban", { hours: Number(hours.value) || null, reason: reason.value }) }, "Ban"),
    el("button", { onclick: closeModal }, "Cancel")));
}

function muteDialog(name) {
  const minutes = el("select", {}, [["15", "15 minutes"], ["60", "1 hour"], ["1440", "1 day"], ["10080", "1 week"]]
    .map(([v, l]) => el("option", { value: v }, l)));
  const reason = el("input", { placeholder: "Reason", maxlength: "300" });
  modal(el("h2", {}, "Mute " + name), minutes, reason, el("div", { class: "actions" },
    el("button", { class: "danger", onclick: () => act(name, "mute", { minutes: Number(minutes.value), reason: reason.value }) }, "Mute"),
    el("button", { onclick: closeModal }, "Cancel")));
}

function roleDialog(name) {
  const role = el("select", {}, ["player", "moderator", "admin"].map((r) => el("option", { value: r }, r)));
  modal(el("h2", {}, "Role of " + name), role, el("div", { class: "actions" },
    el("button", { class: "primary", onclick: () => act(name, "role", { role: role.value }) }, "Save"),
    el("button", { onclick: closeModal }, "Cancel")));
}

function passwordDialog(name) {
  const pass = el("input", { type: "password", placeholder: "New password (8+ characters)", autocomplete: "new-password" });
  modal(el("h2", {}, "New password for " + name), pass, el("p", { class: "muted" }, "Signs the player out everywhere."),
    el("div", { class: "actions" },
      el("button", { class: "danger", onclick: () => act(name, "password", { password: pass.value }) }, "Set password"),
      el("button", { onclick: closeModal }, "Cancel")));
}

function shopDialog(name) {
  const kind = el("select", {}, [["coins", "Give / remove coins"], ["item", "Give an item"], ["vip", "Give VIP days"]]
    .map(([v, l]) => el("option", { value: v }, l)));
  const amount = el("input", { placeholder: "Coins (negative removes) · item code · days" });
  const reason = el("input", { placeholder: "Reason (kept in the audit log)", maxlength: "300" });
  modal(el("h2", {}, "Shop: " + name), kind, amount, reason, el("div", { class: "actions" },
    el("button", { class: "primary", onclick: () => {
      const v = amount.value.trim();
      const body = { reason: reason.value };
      if (kind.value === "coins") body.coins = Number(v);
      if (kind.value === "item") body.item = v;
      if (kind.value === "vip") body.days = Number(v);
      act(name, kind.value, body);
    } }, "Apply"),
    el("button", { onclick: closeModal }, "Cancel")));
}

/// A bar for a chart (sized from script: the panel allows no inline styles).
function barOf(fraction) {
  const b = el("div", { class: "bar" });
  b.style.width = Math.round(Math.max(0, Math.min(1, fraction)) * 100) + "%";
  return b;
}

/// A stored game as a file (the strokes; staff watch it in the game from the
/// player's profile).
async function downloadReplay(id) {
  try {
    const r = await api("/replay/" + id);
    const blob = new Blob([JSON.stringify(r, null, 2)], { type: "application/json" });
    const a = el("a", { href: URL.createObjectURL(blob), download: "snookered-match-" + id + ".json" });
    document.body.append(a);
    a.click();
    a.remove();
  } catch (e) { toast(e.message); }
}

async function showUser(name) {
  view = "users";
  document.querySelectorAll("nav button").forEach((b) => b.classList.toggle("active", b.dataset.view === "users"));
  let d;
  try { d = await api("/users/" + encodeURIComponent(name)); } catch (e) { toast(e.message); return; }
  const u = d.user;
  const now = Date.now() / 1000;
  const banned = u.banned_until && u.banned_until > now;
  const muted = u.muted_until && u.muted_until > now;
  const isAdmin = me && me.role === "admin";
  const history = d.history.map((h) => el("tr", {},
    el("td", {}, fmtDate(h.at)), el("td", {}, h.opponent), el("td", {}, h.won ? "won" : "lost"),
    el("td", {}, h.reason.replaceAll("_", " ")), el("td", {}, h.rated ? (h.delta > 0 ? "+" : "") + h.delta : "casual"),
    el("td", {}, h.rating_after),
    el("td", {}, h.replay ? el("span", { class: "links" },
      el("a", { href: "#", onclick: (e) => { e.preventDefault(); openViewer("/replay/" + h.match_id + "/view"); } }, "Watch"),
      el("a", { href: "#", title: "Download the strokes (JSON)", onclick: (e) => { e.preventDefault(); downloadReplay(h.match_id); } }, "#" + h.match_id)) : "")));
  const reports = d.reports.map((r) => el("div", { class: "card report" },
    el("div", { class: "head" }, el("b", {}, r.reason + " · by " + r.reporter), el("span", { class: "muted" }, fmtDate(r.created_at))),
    r.details ? el("p", {}, r.details) : null, gameBlock(r), r.context ? el("pre", {}, r.context) : null));
  $("#main").replaceChildren(el("div", {},
    el("div", { class: "actions" }, el("button", { onclick: () => setView("users") }, "← Players")),
    el("div", { class: "card" },
      el("h2", {}, u.username, " ", statusTags(u)),
      el("p", {}, "Rating ", el("b", {}, u.rating), " (" + u.tier + ", peak " + u.peak_rating + ") · ",
        u.games + " rated games, " + u.wins + " wins · " + u.casual_games + " casual"),
      el("p", { class: "muted" }, "Country " + (u.country || "—") + " · joined " + fmtDate(u.created_at) + " · last seen " + ago(u.last_seen)),
      el("p", { class: "muted" }, "Recovery e-mail: " + (u.recovery_email || "—") + " · e-mail of the last payment: " + (d.payment_email || "—") +
        " · games left unfinished: " + u.abandons + (d.career && d.career.shots ? " · potting " + Math.round(d.career.potting_shots * 100 / d.career.shots) + "%, best run " + d.career.best_run : "")),
      u.deleted_at ? el("p", { class: "error" }, "Account deleted " + fmtDate(u.deleted_at) + " (anonymised).") : null,
      banned ? el("p", { class: "error" }, "Banned " + until(u.banned_until) + (u.ban_reason ? ": " + u.ban_reason : "")) : null,
      muted ? el("p", { class: "error" }, "Muted until " + fmtDate(u.muted_until)) : null,
      el("div", { class: "actions" },
        banned ? el("button", { onclick: () => act(u.username, "unban") }, "Unban") : el("button", { class: "danger", onclick: () => banDialog(u.username) }, "Ban…"),
        muted ? el("button", { onclick: () => act(u.username, "unmute") }, "Unmute") : el("button", { onclick: () => muteDialog(u.username) }, "Mute…"),
        u.online ? el("button", { onclick: () => act(u.username, "kick", { reason: prompt("Reason (optional)") || "" }) }, "Disconnect") : null,
        el("button", { onclick: () => act(u.username, "logout-all") }, "Sign out everywhere"),
        isAdmin ? el("button", { onclick: () => roleDialog(u.username) }, "Role…") : null,
        isAdmin ? el("button", { onclick: () => passwordDialog(u.username) }, "Reset password…") : null,
        isAdmin ? el("button", { onclick: () => shopDialog(u.username) }, "Shop…") : null)),
    el("h2", {}, "Wallet"),
    el("p", {}, el("b", {}, d.wallet.coins + " coins"), " · VIP ",
      d.wallet.vip_until > now ? "until " + fmtDate(d.wallet.vip_until) : "no",
      " · items: " + (d.wallet.owned.join(", ") || "none")),
    d.payments.length ? table(["When", "Kind", "Coins", "Amount", "Refunded", "Id", ""], d.payments.map((p) => el("tr", {},
      el("td", {}, fmtDate(p.at)), el("td", {}, p.kind, p.test ? el("span", { class: "tag mute" }, " test") : null),
      el("td", {}, p.coins),
      el("td", {}, (p.amount / 100).toFixed(2) + " " + p.currency.toUpperCase()), el("td", {}, p.refunded ? "yes" : ""),
      el("td", {}, p.id),
      el("td", {}, me && me.role === "admin" ? el("button", {
        onclick: async () => {
          try {
            await api("/payments/" + encodeURIComponent(p.id) + "/test", { method: "POST", body: { test: !p.test } });
            toast(p.test ? "Marked as real money." : "Marked as a test payment."); showUser(name);
          } catch (e) { toast(e.message); }
        },
      }, p.test ? "Mark as real" : "Mark as test") : null)))) : el("p", { class: "muted" }, "No payments."),
    d.ledger.length ? table(["When", "Coins", "Why"], d.ledger.map((l) => el("tr", {},
      el("td", {}, fmtDate(l.at)), el("td", {}, (l.delta > 0 ? "+" : "") + l.delta), el("td", {}, l.reason)))) : null,
    el("h2", {}, "Reports about this player (" + d.reports.length + ")"),
    reports.length ? reports : el("p", { class: "muted" }, "None."),
    el("h2", {}, "Recent games"),
    history.length ? table(["When", "Opponent", "Result", "How", "Rating", "After", "Replay"], history) : el("p", { class: "muted" }, "None."),
    el("h2", {}, "Achievements (" + d.achievements.length + ")"),
    el("p", { class: "muted" }, d.achievements.map((a) => a[0]).join(", ") || "None.")));
}

/// An amount in a currency's smallest unit, for people.
function money(cents, currency) {
  try { return new Intl.NumberFormat(undefined, { style: "currency", currency: currency.toUpperCase() }).format(cents / 100); }
  catch (_) { return (cents / 100).toFixed(2) + " " + currency.toUpperCase(); }
}

/// A quiet note where a chart would be.
function empty(text) {
  return el("p", { class: "muted empty" }, text);
}

const SVG = "http://www.w3.org/2000/svg";
function svg(tag, attrs = {}, ...children) {
  const e = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return e;
}

/// The smallest 1, 2 or 5 × 10ⁿ at or above `v`.
function niceMax(v) {
  if (!(v > 0)) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  for (const k of [1, 2, 5, 10]) if (k * p >= v) return k * p;
  return 10 * p;
}

/// Bars (and optionally a line) over evenly spaced columns, in SVG. Each
/// column has a tooltip with its values.
function chart({ labels, titles, bars, line, every = 6, height = 190, unit = "", width = 960 }) {
  const W = width, H = height, left = 46, right = 14, top = 12, bottom = every ? 24 : 8;
  const n = labels.length;
  const values = bars.values.concat(line ? line.values : []);
  const max = niceMax(Math.max(0, ...values));
  const fmt = bars.format || ((v) => String(Math.round(v * 10) / 10));
  const step = (W - left - right) / Math.max(1, n);
  const x = (k) => left + step * k + step / 2;
  const y = (v) => top + (H - top - bottom) * (1 - v / max);
  const g = svg("svg", { viewBox: "0 0 " + W + " " + H, class: "chart", role: "img", "aria-label": bars.name + (line ? " and " + line.name : "") });
  for (const t of [0, 0.5, 1]) {
    const yy = y(max * t);
    g.append(svg("line", { x1: left, x2: W - right, y1: yy, y2: yy, class: "grid-line" }),
      svg("text", { x: left - 8, y: yy + 4, class: "axis", "text-anchor": "end" }, fmt(max * t)));
  }
  const bw = Math.max(1.5, step * 0.68);
  bars.values.forEach((v, k) => {
    if (v > 0) {
      const h = Math.max(1.5, y(0) - y(v));
      g.append(svg("rect", { x: x(k) - bw / 2, y: y(0) - h, width: bw, height: h, rx: Math.min(3, bw / 3), class: "col " + bars.cls }));
    }
  });
  if (line) {
    const pts = line.values.map((v, k) => x(k).toFixed(1) + "," + y(v).toFixed(1)).join(" ");
    g.append(svg("polyline", { points: pts, class: "line " + line.cls }));
    line.values.forEach((v, k) => { if (v > 0) g.append(svg("circle", { cx: x(k), cy: y(v), r: 2.6, class: "dot " + line.cls })); });
  }
  if (every) {
    labels.forEach((l, k) => {
      if (k % every === 0 || k === n - 1) g.append(svg("text", { x: x(k), y: H - 6, class: "axis", "text-anchor": "middle" }, l));
    });
  }
  // Tooltips over whole columns.
  labels.forEach((_, k) => {
    let tip = (titles[k] || labels[k]) + "\n" + bars.name + ": " + fmt(bars.values[k]) + unit;
    if (line) tip += "\n" + line.name + ": " + fmt(line.values[k]);
    g.append(svg("rect", { x: left + step * k, y: top, width: step, height: H - top - bottom, class: "hit" }, svg("title", {}, tip)));
  });
  const key = (name, cls, kind) => el("span", { class: "key" }, el("i", { class: kind + " " + cls }), name);
  return el("div", { class: "chart-wrap" },
    el("div", { class: "legend" }, key(bars.name, bars.cls, "sw-bar"), line ? key(line.name, line.cls, "sw-line") : null),
    g);
}

/// The game that came with a report.
function gameBlock(r) {
  const g = r.game;
  if (!g) return el("div", { class: "game none" }, el("span", { class: "muted" }, "No game between them in the 24 hours before the report."));
  if (g.kind === "gone") {
    return el("div", { class: "game gone" }, el("span", { class: "muted" },
      "They were playing when this was sent, but that game was never stored (the server restarted and nobody came back to it)."));
  }
  const live = g.kind === "live";
  const when = live ? (g.finished ? "just ended" : "in progress when reported") : "ended " + fmtDate(g.at);
  return el("div", { class: "game" },
    el("div", { class: "game-info" },
      el("b", {}, live ? "Game in progress attached" : "Game attached"),
      el("span", {}, g.players[0] + " vs " + g.players[1]),
      el("span", { class: "muted" }, [when, g.rated ? "rated" : "casual", g.shots + (g.shots === 1 ? " stroke" : " strokes")].join(" · "))),
    el("div", { class: "game-actions" },
      el("button", { class: "primary", onclick: () => openViewer(live ? "/live/" + g.id + "/view" : "/replay/" + g.id + "/view") },
        live ? "Watch so far" : "Watch replay"),
      live ? null : el("button", { class: "ghost", title: "The strokes as a JSON file", onclick: () => downloadReplay(g.id) }, "Download")));
}

// ---------------------------------------------------------------------------
// The replay viewer: the table from above, every shot as the server played it.

let viewer = null;
const BALL_COLORS = ["#f4efe2", "#f2c200", "#2051c9", "#d42a2a", "#6c2fa3", "#f07a1a", "#1d8a3a", "#8a2323", "#141414"];
const ballColor = (n) => BALL_COLORS[n > 8 ? n - 8 : n];

function closeViewer() {
  if (!viewer) return;
  cancelAnimationFrame(viewer.raf);
  clearTimeout(viewer.hold);
  viewer.box.remove();
  viewer = null;
}

async function openViewer(path) {
  closeViewer();
  const card = el("div", { class: "viewer-card card" }, el("div", { class: "viewer-wait" }, el("span", { class: "spinner" }), "Rebuilding the game…"));
  const box = el("div", { class: "viewer", role: "dialog", "aria-modal": "true" }, card);
  box.addEventListener("click", (e) => { if (e.target === box) closeViewer(); });
  document.body.append(box);
  viewer = { box, raf: 0 };
  let data;
  try { data = await api(path); } catch (e) {
    if (viewer && viewer.box === box) {
      card.replaceChildren(el("p", { class: "error" }, e.message), el("div", { class: "actions" }, el("button", { onclick: closeViewer }, "Close")));
    }
    return;
  }
  if (!viewer || viewer.box !== box) return;
  buildViewer(card, data, path);
}

function strokeSummary(s, names) {
  const who = names[s.seat] || "Player " + (s.seat + 1);
  if (s.kind === "resign") return [who, "resigned"];
  if (s.kind === "abandon") return [who, "left the game"];
  if (s.kind === "time_foul") return [who, "ran out of time"];
  const made = s.pocketed.filter((p) => p.ball !== 0).map((p) => p.ball);
  const parts = [];
  if (s.break) parts.push("break");
  parts.push(made.length ? "made " + made.join(", ") : s.first === null ? "hit nothing" : "made nothing");
  if (s.fouls.length) parts.push("foul");
  return [who, parts.join(" · ")];
}

function strokeDetails(s) {
  if (s.kind !== "shot") return [];
  const out = [];
  for (const p of s.pocketed) out.push((p.ball === 0 ? "Cue ball" : p.ball + "-ball") + " in the " + p.pocket);
  for (const f of s.fouls) out.push("Foul: " + f);
  if (s.called) out.push("8-ball called in the " + s.called);
  if (s.in_hand) out.push("ball in hand");
  if (s.timeout) out.push("played by the shot clock");
  out.push("power " + s.power + "%");
  return out;
}

function buildViewer(card, data, path) {
  const t = data.table;
  const m = data.match;
  const names = m.players;
  const R = t.ball_radius;
  const rail = 120, L = t.length, Wd = t.width;
  const vb = [-L / 2 - rail, -Wd / 2 - rail, L + 2 * rail, Wd + 2 * rail];
  const board = svg("svg", { viewBox: vb.join(" "), class: "table-svg", role: "img", "aria-label": "The table from above" });
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
  for (let k = 1; k < 4; k++) {
    for (const sx of [-1, 1]) board.append(svg("circle", { cx: sx * (L / 2 + rail * 0.62), cy: -Wd / 2 + k * Wd / 4, r: 7, class: "t-diamond" }));
  }
  for (const c of t.cushions) board.append(svg("line", { x1: c[0], y1: -c[1], x2: c[2], y2: -c[3], class: "t-jaw" }));
  for (const p of t.pockets) board.append(svg("circle", { cx: p.x, cy: -p.y, r: p.r, class: "t-pocket" }, svg("title", {}, p.name)));
  const balls = [];
  for (let n = 0; n < 16; n++) {
    const g = svg("g", { class: "ball" });
    const color = ballColor(n);
    if (n > 8) {
      defs.append(svg("clipPath", { id: "vb" + n }, svg("circle", { r: R })));
      g.append(svg("circle", { r: R, fill: "#f4efe2" }),
        svg("rect", { x: -R, y: -R * 0.55, width: 2 * R, height: R * 1.1, fill: color, "clip-path": "url(#vb" + n + ")" }));
    } else {
      g.append(svg("circle", { r: R, fill: color }));
    }
    g.append(svg("circle", { r: R, class: "ball-edge" }));
    if (n > 0) {
      g.append(svg("circle", { r: R * 0.5, class: "ball-disc" }),
        svg("text", { y: R * 0.22, "text-anchor": "middle", class: "ball-num" }, n));
    }
    g.append(svg("title", {}, n === 0 ? "Cue ball" : n + "-ball"));
    balls.push(g);
    board.append(g);
  }

  const strokes = data.strokes;
  const st = { k: 0, f: 0, pos: [], playing: false, speed: 1, acc: 0, last: 0 };
  const startOf = (k) => (k < strokes.length ? strokes[k].start : data.end).map((p) => (p ? p.slice() : null));
  const apply = (frame) => {
    for (let i = 0; i + 2 < frame.length; i += 3) {
      const b = frame[i];
      st.pos[b] = frame[i + 1] === null ? null : [frame[i + 1], frame[i + 2]];
    }
  };
  const frames = (k) => (k < strokes.length ? strokes[k].frames : []);
  const draw = () => {
    st.pos.forEach((p, n) => {
      if (p) {
        balls[n].setAttribute("transform", "translate(" + p[0] + "," + -p[1] + ")");
        balls[n].removeAttribute("visibility");
      } else {
        balls[n].setAttribute("visibility", "hidden");
      }
    });
    const fr = frames(st.k);
    scrub.max = String(fr.length);
    scrub.value = String(st.f);
    scrub.disabled = fr.length === 0;
    play.textContent = st.playing ? "❚❚ Pause" : "▶ Play";
    prev.disabled = st.k === 0 && st.f === 0;
    next.disabled = st.k >= strokes.length;
    const s = strokes[st.k];
    if (s) {
      const [who, what] = strokeSummary(s, names);
      status.replaceChildren(el("b", {}, "Stroke " + (st.k + 1) + " of " + strokes.length), " · ", who, " · ", what,
        el("div", { class: "muted details" }, strokeDetails(s).join(" · ")));
    } else {
      status.replaceChildren(el("b", {}, "End of the game"), " · ", over());
    }
    list.querySelectorAll("li").forEach((li, k) => li.classList.toggle("current", k === st.k));
    const cur = list.children[Math.min(st.k, list.children.length - 1)];
    if (cur && st.playing) cur.scrollIntoView({ block: "nearest" });
  };
  const over = () => {
    if (data.over) return (names[data.over.winner] || "?") + " won (" + data.over.reason.replaceAll("_", " ") + ")";
    return m.live ? "still being played" : "unfinished";
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
    while (st.acc >= 1 && st.f < fr.length) {
      apply(fr[st.f]);
      st.f++;
      st.acc -= 1;
      moved = true;
    }
    if (moved) draw();
    if (st.f >= fr.length) {
      // A breath between strokes, then the next one.
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
  const start = () => {
    if (st.k >= strokes.length) seek(0, 0);
    st.playing = true;
    st.last = 0;
    st.acc = 0;
    draw();
    cancelAnimationFrame(viewer.raf);
    viewer.raf = requestAnimationFrame(tick);
  };
  const toggle = () => (st.playing ? pause() : start());
  const stepFrame = (d) => {
    pause();
    const fr = frames(st.k);
    if (d > 0) {
      if (st.f < fr.length) seek(st.k, st.f + 1);
      else if (st.k < strokes.length) seek(st.k + 1, 0);
    } else if (st.f > 0) seek(st.k, st.f - 1);
    else if (st.k > 0) seek(st.k - 1, frames(st.k - 1).length);
  };
  const stepStroke = (d) => {
    pause();
    if (d < 0 && st.f > 0) seek(st.k, 0);
    else seek(st.k + d, 0);
  };

  const prev = el("button", { title: "Previous stroke (←)", onclick: () => stepStroke(-1) }, "⏮");
  const back = el("button", { title: "One frame back (,)", onclick: () => stepFrame(-1) }, "‹");
  const play = el("button", { class: "primary play", title: "Play / pause (space)", onclick: toggle }, "▶ Play");
  const fwd = el("button", { title: "One frame forward (.)", onclick: () => stepFrame(1) }, "›");
  const next = el("button", { title: "Next stroke (→)", onclick: () => stepStroke(1) }, "⏭");
  const speed = el("select", { title: "Speed", onchange: () => { st.speed = Number(speed.value); } },
    [["0.25", "¼×"], ["0.5", "½×"], ["1", "1×"], ["2", "2×"], ["4", "4×"]].map(([v, l]) => el("option", { value: v }, l)));
  speed.value = "1";
  const scrub = el("input", { type: "range", min: "0", max: "0", value: "0", class: "scrub", "aria-label": "Position in the stroke" });
  scrub.addEventListener("input", () => { pause(); seek(st.k, Number(scrub.value)); });
  const status = el("div", { class: "viewer-status" });
  const list = el("ol", { class: "stroke-list" }, strokes.map((s, k) => {
    const [who, what] = strokeSummary(s, names);
    const foul = s.kind !== "shot" || s.fouls.length > 0;
    return el("li", { class: foul ? "foul" : "", onclick: () => { pause(); seek(k, 0); } },
      el("span", { class: "num" }, k + 1), el("span", { class: "who seat" + s.seat }, who), el("span", { class: "what" }, what));
  }));
  const title = el("div", { class: "viewer-title" },
    el("b", {}, names[0] + " vs " + names[1]),
    el("span", { class: "muted" }, [m.live ? "in progress" : m.rated ? "rated" : "casual", m.live ? "so far" : fmtDate(m.at),
      m.id ? "#" + m.id : null].filter(Boolean).join(" · ")));
  const close = el("button", { class: "ghost", title: "Close (Esc)", onclick: closeViewer }, "✕");
  card.replaceChildren(
    el("div", { class: "viewer-head" }, title, close),
    el("div", { class: "viewer-body" },
      el("div", { class: "viewer-stage" }, board, scrub,
        el("div", { class: "viewer-controls" }, prev, back, play, fwd, next, speed), status),
      el("div", { class: "viewer-side" }, el("div", { class: "muted side-head" }, strokes.length + " strokes · " + over()), list)),
    ...(data.error ? [el("p", { class: "error" }, "The replay stops early: " + data.error)] : []));
  viewer.keys = (e) => {
    if (e.target && ["INPUT", "SELECT", "TEXTAREA"].includes(e.target.tagName) && e.target !== scrub) return;
    if (e.key === "Escape") closeViewer();
    else if (e.key === " ") { e.preventDefault(); toggle(); }
    else if (e.key === "ArrowLeft") { e.preventDefault(); stepStroke(-1); }
    else if (e.key === "ArrowRight") { e.preventDefault(); stepStroke(1); }
    else if (e.key === ",") stepFrame(-1);
    else if (e.key === ".") stepFrame(1);
    else return;
  };
  seek(0, 0);
  play.focus();
}

document.addEventListener("keydown", (e) => { if (viewer && viewer.keys) viewer.keys(e); else if (viewer && e.key === "Escape") closeViewer(); });

function signOut(callServer = true) {
  if (callServer && token) api("/logout", { method: "POST" }).catch(() => {});
  token = "";
  me = null;
  sessionStorage.removeItem("snookered-admin");
  $("#app").classList.add("hidden");
  $("#login").classList.remove("hidden");
}

async function start() {
  if (!token) { signOut(false); return; }
  try {
    me = await api("/me");
    $("#me").textContent = me.username + " · " + me.role;
    $("#me").className = me.role === "admin" ? "admin" : "";
    $("#login").classList.add("hidden");
    $("#app").classList.remove("hidden");
    render();
  } catch (_) { signOut(false); }
}

$("#login-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  $("#login-error").textContent = "";
  try {
    const d = await api("/login", { method: "POST", body: { username: $("#login-user").value, password: $("#login-pass").value } });
    token = d.token;
    sessionStorage.setItem("snookered-admin", token);
    $("#login-pass").value = "";
    start();
  } catch (err) { $("#login-error").textContent = err.message; }
});
$("#logout").addEventListener("click", () => signOut(true));
// The metrics page keeps itself current.
setInterval(() => {
  if (token && view === "metrics" && !viewer && document.visibilityState === "visible" && $("#main .metrics")) render();
}, 60000);
document.querySelectorAll("nav button").forEach((b) => b.addEventListener("click", () => setView(b.dataset.view)));
start();
