"use strict";
(() => {
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.prototype.slice.call((r || document).querySelectorAll(s));

  const now = () => new Date();
  const fmtTime = (d) =>
    d.toTimeString().slice(0, 8).replace(/:/g, ":");
  const esc = (v) =>
    String(v == null ? "" : v)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  const uid = () =>
    (crypto.randomUUID
      ? crypto.randomUUID()
      : "id-" + Date.now() + "-" + Math.random().toString(36).slice(2, 9));
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));

  const store = {
    brain: "IDLE",
    zone: "CORE",
    connected: false,
    startedAt: Date.now(),
    events: [],
    errors: [],
    rate: { n: 0, at: now() },
    lastTab: localStorage.getItem("james.tab") || "overview",
    messages: JSON.parse(localStorage.getItem("james.chat") || "[]"),
    settings: Object.assign(
      { showDetails: true, notifications: true, persistChat: true, timestamps: true },
      JSON.parse(localStorage.getItem("james.settings") || "{}")
    ),
    decayToken: 0,
    lastTaskStates: {},
    intent: null,
    mode: "JETZT",
    showPanels: [],
    openPanels: [],
    actions: [],
    securityDecision: "",
    activity: 0,
  };

  const BRAIN = {
    name: {
      IDLE: "IDLE", LISTENING: "LISTENING", UNDERSTANDING: "UNDERSTANDING", THINKING: "THINKING",
      PLANNING: "PLANNING", SEARCHING: "SEARCHING", EXECUTING: "EXECUTING", VERIFYING: "VERIFYING",
      COMPLETED: "COMPLETED", WAITING: "WAITING", LEARNING: "LEARNING", WARNING: "WARNING",
      ERROR: "ERROR", RECOVERING: "RECOVERING", RESOURCE_LIMITED: "RESOURCE LIMITED",
      SECURITY_LOCK: "SECURITY LOCK", SLEEPING: "SLEEPING", OFFLINE: "OFFLINE",
    },
    profile: {
      IDLE: { detail: "Ein ruhiges System, bereit zu denken.", zone: "CORE", speed: 0.05, intensity: 0.08, density: 0.35, focus: "BREIT", glow: 0.05, signal: 0.015 },
      LISTENING: { detail: "JAMES hört auf deine Eingabe.", zone: "INPUT", speed: 0.12, intensity: 0.3, density: 0.45, focus: "TIGHT", glow: 0.1, signal: 0.02 },
      UNDERSTANDING: { detail: "Die Anfrage wird zerlegt und verstanden.", zone: "REASONING", speed: 0.22, intensity: 0.45, density: 0.55, focus: "TIGHT", glow: 0.14, signal: 0.04 },
      THINKING: { detail: "Kern arbeitet still an einer Anfrage.", zone: "REASONING", speed: 0.32, intensity: 0.55, density: 0.6, focus: "TIGHT", glow: 0.22, signal: 0.06 },
      PLANNING: { detail: "Ein Plan wird in Schritte zerlegt und belegt.", zone: "PLANNING", speed: 0.26, intensity: 0.62, density: 0.68, focus: "TIGHT", glow: 0.28, signal: 0.08 },
      SEARCHING: { detail: "JAMES durchsucht Quellen nach relevanten Fakten.", zone: "MEMORY", speed: 0.52, intensity: 0.5, density: 0.75, focus: "BREIT", glow: 0.18, signal: 0.09 },
      EXECUTING: { detail: "Aktionen werden ausgeführt und geprüft.", zone: "ACTION", speed: 0.6, intensity: 0.85, density: 0.9, focus: "TIGHT", glow: 0.42, signal: 0.14 },
      VERIFYING: { detail: "Ergebnisse werden geprüft und bestätigt.", zone: "VERIFY", speed: 0.5, intensity: 0.72, density: 0.8, focus: "TIGHT", glow: 0.34, signal: 0.1 },
      COMPLETED: { detail: "Anfrage erfolgreich abgeschlossen.", zone: "OUTPUT", speed: 0.18, intensity: 0.3, density: 0.5, focus: "BREIT", glow: 0.12, signal: 0.03 },
      WAITING: { detail: "JAMES wartet auf Eingabe oder Freigabe.", zone: "INPUT", speed: 0.1, intensity: 0.28, density: 0.5, focus: "TIGHT", glow: 0.1, signal: 0.02 },
      LEARNING: { detail: "Neues Wissen wird gespeichert und konsolidiert.", zone: "MEMORY", speed: 0.2, intensity: 0.44, density: 0.7, focus: "BREIT", glow: 0.16, signal: 0.05 },
      WARNING: { detail: "Auffälligkeit bemerkt. Bordmechanismen greifen ein.", zone: "VERIFY", speed: 0.75, intensity: 0.8, density: 0.85, focus: "TIGHT", glow: 0.5, signal: 0.1 },
      ERROR: { detail: "Ein Fehler ist aufgetreten. JAMES versucht die Lage zu stabilisieren.", zone: "RECOVERY", speed: 0.95, intensity: 0.95, density: 0.95, focus: "TIGHT", glow: 0.65, signal: 0.2 },
      RECOVERING: { detail: "JAMES stellt konsistenten Zustand wieder her.", zone: "RECOVERY", speed: 0.42, intensity: 0.5, density: 0.5, focus: "TIGHT", glow: 0.24, signal: 0.07 },
      RESOURCE_LIMITED: { detail: "Ressourcengrenze erreicht. JAMES drosselt die Last.", zone: "CORE", speed: 0.12, intensity: 0.38, density: 0.5, focus: "TIGHT", glow: 0.12, signal: 0.02 },
      SECURITY_LOCK: { detail: "Sicherheitsprüfung aktiv. Aktion blockiert oder wartet auf Freigabe.", zone: "VERIFY", speed: 0.68, intensity: 0.88, density: 0.9, focus: "TIGHT", glow: 0.58, signal: 0.16 },
      SLEEPING: { detail: "System liegt im Standby. Der Kern bleibt wach.", zone: "CORE", speed: 0.02, intensity: 0.05, density: 0.2, focus: "BREIT", glow: 0.03, signal: 0.003 },
      OFFLINE: { detail: "Keine Verbindung zum System.", zone: "CORE", speed: 0.02, intensity: 0.06, density: 0.15, focus: "BREIT", glow: 0.02, signal: 0.002 },
    },
  };

  const EVENT_TO_BRAIN = [
    [["task.completed", "task.done", "capability.executed", "core.ready"], "RECOVERING"],
    [["error"], "ERROR"],
    [["warn", "warning", "security", "threat", "approval"], "WARNING"],
    [["learned", "memory.consolidated"], "LEARNING"],
    [["search"], "SEARCHING"],
    [["plan"], "PLANNING"],
    [["task.started", "task.running", "execute", "capability.run"], "EXECUTING"],
    [["ai", "thinking"], "THINKING"],
    [["wait", "blocked", "paused"], "WAITING"],
  ];

  const ZONES = [
    ["INPUT", 0], ["MEMORY", 60], ["REASONING", 120], ["PLANNING", 180], ["CORE", 0],
    ["ACTION", 240], ["OUTPUT", 300], ["VERIFY", 45], ["RECOVERY", 315],
  ];

  const NODES = [
    ["input", "INPUT", "EINGABE"], ["audio", "INPUT", "SPRACHE"],
    ["memory", "MEMORY", "LANGSAM"], ["working", "MEMORY", "ARBEITS"],
    ["reason", "REASONING", "LOGIK"], ["deduce", "REASONING", "SCHLUSS"],
    ["planner", "PLANNING", "STRATEGIE"], ["tasks", "PLANNING", "SCHRITTE"],
    ["core", "CORE", "KERN"], ["bus", "CORE", "BUS"],
    ["cap", "ACTION", "FÄHIGKEIT"], ["browser", "ACTION", "NETZ"],
    ["voice", "OUTPUT", "STIMME"], ["textout", "OUTPUT", "TEXT"],
    ["verify", "VERIFY", "PRÜFUNG"], ["shield", "VERIFY", "SCHUTZ"],
    ["recover", "RECOVERY", "REPARATUR"], ["audit", "RECOVERY", "AUDIT"],
  ];

  const zoneAngle = {};
  ZONES.forEach((z, i) => (zoneAngle[z[0]] = (i * 40 + 108) * (Math.PI / 180)));

  let canvas, ctx, W = 0, H = 0, dpr = 1, rafId = 0;
  let brainPhase = 0, activityMix = 0, glowMix = 0;

  function layout() {
    if (!canvas) return;
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    const rect = canvas.getBoundingClientRect();
    W = Math.max(1, rect.width);
    H = Math.max(1, rect.height);
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    ctx = canvas.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function zoneCenter(zone) {
    const cx = W / 2, cy = H / 2;
    const angle = zoneAngle[zone] || 0;
    const r = Math.min(W, H) * 0.30;
    if (zone === "CORE") return [cx, cy, 0];
    return [cx + Math.cos(angle) * r, cy + Math.sin(angle) * r, r * 0.44];
  }

  function nodePosition(node) {
    const [zx, zy, zr] = zoneCenter(node.zone);
    const idx = NODES.filter((n) => n[1] === node.zone).indexOf(node);
    const count = NODES.filter((n) => n[1] === node.zone).length;
    const a = (idx / Math.max(1, count)) * Math.PI * 2 + (node.zone === "CORE" ? Math.PI / 4 : 0);
    const r = zr * 0.72;
    return [zx + Math.cos(a) * r, zy + Math.sin(a) * r];
  }

  const pulseByZone = {};
  const nodeCache = NODES.map((n) => {
    const node = { id: n[0], zone: n[1], label: n[2], rad: n[1] === "CORE" ? 5 : 3.2, activity: 0, pulse: 0 };
    pulseByZone[n[1]] = pulseByZone[n[1]] || [];
    pulseByZone[n[1]].push(node);
    return node;
  });

  let signals = [];
  let prefersReducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  window.matchMedia("(prefers-reduced-motion: reduce)").addEventListener("change", (e) => { prefersReducedMotion = e.matches; });

  /* ---------- Semantic signal flows per brain state ----------
   * Each state defines directed flows between zones that represent real
   * information paths. No random walks - every signal has a causal direction. */
const STATE_FLOWS = {
    IDLE: [],
    LISTENING: [
      { from: "input", to: "core", kind: "in", weight: 1.0 },
      { from: "audio", to: "input", kind: "in", weight: 0.3 },
    ],
    UNDERSTANDING: [
      { from: "input", to: "reason", kind: "in", weight: 0.8 },
      { from: "reason", to: "core", kind: "in", weight: 0.6 },
      { from: "core", to: "deduce", kind: "walk", weight: 0.4 },
    ],
    THINKING: [
      { from: "core", to: "reason", kind: "walk", weight: 0.9 },
      { from: "reason", to: "core", kind: "walk", weight: 0.9 },
      { from: "core", to: "deduce", kind: "walk", weight: 0.3 },
    ],
    PLANNING: [
      { from: "core", to: "planner", kind: "out", weight: 1.0 },
      { from: "planner", to: "tasks", kind: "out", weight: 0.8 },
      { from: "tasks", to: "core", kind: "walk", weight: 0.4 },
    ],
    SEARCHING: [
      { from: "core", to: "memory", kind: "out", weight: 0.7 },
      { from: "memory", to: "working", kind: "walk", weight: 0.6 },
      { from: "working", to: "reason", kind: "in", weight: 0.5 },
      { from: "memory", to: "reason", kind: "in", weight: 0.5 },
    ],
    EXECUTING: [
      { from: "core", to: "cap", kind: "out", weight: 1.0 },
      { from: "cap", to: "browser", kind: "out", weight: 0.7 },
      { from: "browser", to: "voice", kind: "out", weight: 0.5 },
    ],
    VERIFYING: [
      { from: "voice", to: "verify", kind: "in", weight: 1.0 },
      { from: "verify", to: "shield", kind: "walk", weight: 0.6 },
      { from: "verify", to: "core", kind: "in", weight: 0.8 },
    ],
    COMPLETED: [
      { from: "core", to: "voice", kind: "out", weight: 0.6 },
      { from: "voice", to: "textout", kind: "out", weight: 0.4 },
    ],
    WAITING: [
      { from: "input", to: "core", kind: "in", weight: 0.4 },
    ],
    LEARNING: [
      { from: "core", to: "memory", kind: "out", weight: 0.7 },
      { from: "memory", to: "working", kind: "walk", weight: 0.5 },
      { from: "working", to: "core", kind: "in", weight: 0.4 },
    ],
    WARNING: [
      { from: "verify", to: "shield", kind: "walk", weight: 0.8 },
      { from: "core", to: "verify", kind: "walk", weight: 0.5 },
      { from: "core", to: "recover", kind: "out", weight: 0.3 },
    ],
    ERROR: [
      { from: "recover", to: "audit", kind: "walk", weight: 0.7 },
      { from: "core", to: "recover", kind: "out", weight: 0.4 },
    ],
    RECOVERING: [
      { from: "recover", to: "core", kind: "in", weight: 0.8 },
      { from: "audit", to: "recover", kind: "walk", weight: 0.5 },
      { from: "core", to: "cap", kind: "out", weight: 0.3 },
    ],
    SLEEPING: [],
    OFFLINE: [],
    RESOURCE_LIMITED: [
      { from: "core", to: "core", kind: "walk", weight: 0.2 },
    ],
    SECURITY_LOCK: [
      { from: "verify", to: "shield", kind: "walk", weight: 0.9 },
      { from: "core", to: "verify", kind: "walk", weight: 0.5 },
    ],
  };

  function getNodeById(id) {
    return nodeCache.find((n) => n.id === id);
  }

  function spawnSignals(profile, dt) {
    if (prefersReducedMotion) return;
    const flows = STATE_FLOWS[store.brain] || [];
    const baseRate = profile.signal * dt * 60;
    for (const flow of flows) {
      const rate = baseRate * flow.weight;
      const count = Math.floor(rate);
      const remainder = rate - count;
      const n = count + (Math.random() < remainder ? 1 : 0);
      for (let i = 0; i < n; i++) {
        const from = getNodeById(flow.from);
        const to = getNodeById(flow.to);
        if (!from || !to) continue;
        const [x1, y1] = nodePosition(from);
        const [x2, y2] = nodePosition(to);
        signals.push({ x: x1, y: y1, tx: x2, ty: y2, t: 0, dur: 0.5 + Math.random() * 0.7, kind: flow.kind, seed: Math.random() });
      }
    }
    signals = signals.filter((s) => s.t < 1);
    if (signals.length > 120) signals.splice(0, signals.length - 120);
  }

  function drawBrain(profile) {
    if (!ctx) return;
    const isSecurityLock = store.brain === "SECURITY_LOCK";
    const isError = store.brain === "ERROR";
    const isRecovering = store.brain === "RECOVERING";
    const isOffline = store.brain === "OFFLINE";
    const isSleeping = store.brain === "SLEEPING";
    const isIdle = store.brain === "IDLE";

    if (!prefersReducedMotion) {
      brainPhase += profile.speed * 0.016;
    }
    activityMix += (profile.intensity - activityMix) * 0.06;
    glowMix += (profile.glow - glowMix) * 0.06;

    const cx = W / 2, cy = H / 2, base = Math.min(W, H);
    ctx.clearRect(0, 0, W, H);

    // Activity field - derived from real activity only
    if (activityMix > 0.02) {
      const ring = ctx.createRadialGradient(cx, cy, 0, cx, cy, base * 0.62);
      ring.addColorStop(0, `rgba(112,231,255,${0.05 + glowMix * 0.09})`);
      ring.addColorStop(0.6, `rgba(112,231,255,${0.012 + glowMix * 0.04})`);
      ring.addColorStop(1, "rgba(112,231,255,0)");
      ctx.fillStyle = ring;
      ctx.beginPath();
      ctx.arc(cx, cy, base * 0.62, 0, Math.PI * 2);
      ctx.fill();
    }

    nodeCache.forEach((n) => { n.pulse *= 0.95; });

    // Signal flow layer
    ctx.lineCap = "round";
    signals.forEach((s) => {
      if (!prefersReducedMotion) s.t += 0.016 / s.dur;
      if (s.t >= 1) return;
      const x = s.x + (s.tx - s.x) * s.t;
      const y = s.y + (s.ty - s.y) * s.t;
      const flow = 0.04 + 0.1 * activityMix;
      ctx.strokeStyle = `rgba(112,231,255,${flow})`;
      ctx.lineWidth = 1 + activityMix;
      ctx.beginPath();
      ctx.moveTo(s.x, s.y);
      ctx.lineTo(x, y);
      ctx.stroke();
      const head = ctx.createRadialGradient(x, y, 0, x, y, 3 + activityMix * 4);
      head.addColorStop(0, s.kind === "out" ? "rgba(225,244,255,.95)" : "rgba(112,231,255,.85)");
      head.addColorStop(1, "rgba(112,231,255,0)");
      ctx.fillStyle = head;
      ctx.beginPath();
      ctx.arc(x, y, 3 + activityMix * 4, 0, Math.PI * 2);
      ctx.fill();
    });

    // Core layer rings - only active zone rings, no global rotation
    const activeZones = new Set();
    (STATE_FLOWS[store.brain] || []).forEach((f) => { activeZones.add(f.from); activeZones.add(f.to); });
    if (profile.zone && profile.zone !== "CORE") activeZones.add(profile.zone);
    activeZones.add("CORE");

    ZONES.forEach((zone) => {
      if (zone[0] === "CORE") return;
      if (!activeZones.has(zone[0])) return;
      const [zx, zy, zr] = zoneCenter(zone[0]);
      const isFocus = zone[0] === profile.zone;
      ctx.strokeStyle = `rgba(112,231,255,${isFocus ? 0.34 : 0.12})`;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(zx, zy, zr, 0, Math.PI * 2);
      ctx.stroke();
      ctx.fillStyle = `rgba(112,231,255,${isFocus ? 0.85 : 0.25})`;
      ctx.font = "7px 'JetBrains Mono', monospace";
      ctx.letterSpacing = "2px";
      ctx.fillText(zone[0], zx - 12, zy - zr - 6);
    });

    // Core pulse - state-specific
    const corePulse = isSecurityLock ? 0.8 + Math.sin(brainPhase * 3) * 0.2
      : isError ? 0.6 + Math.sin(brainPhase * 5) * 0.3
      : isRecovering ? 0.5 + Math.sin(brainPhase * 2) * 0.25
      : isSleeping ? 0.3 + Math.sin(brainPhase * 0.5) * 0.1
      : isIdle ? 0.4 + Math.sin(brainPhase * 0.8) * 0.15
      : 0.6 + Math.sin(brainPhase * 1.2) * 0.2;
    ctx.strokeStyle = `rgba(225,244,255,${corePulse * (0.5 + glowMix * 0.3)})`;
    ctx.lineWidth = isSecurityLock ? 2.2 : 1.4;
    ctx.beginPath();
    ctx.arc(cx, cy, base * 0.18 + Math.sin(brainPhase * (isSecurityLock ? 2.5 : 1.5)) * (isSecurityLock ? 4 : 2), 0, Math.PI * 2);
    ctx.stroke();

    // SECURITY_LOCK ring at VERIFY zone
    if (isSecurityLock) {
      const verifyNode = getNodeById("verify");
      if (verifyNode) {
        const [vx, vy] = nodePosition(verifyNode);
        const r = 28 + Math.sin(brainPhase * 2) * 3;
        ctx.strokeStyle = `rgba(168,140,255,${0.5 + Math.sin(brainPhase * 3) * 0.3})`;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(vx, vy, r, 0, Math.PI * 2);
        ctx.stroke();
      }
    }

    // ERROR isolation ring at RECOVERY zone
    if (isError) {
      const recoverNode = getNodeById("recover");
      if (recoverNode) {
        const [rx, ry] = nodePosition(recoverNode);
        ctx.strokeStyle = `rgba(255,113,133,${0.4 + Math.sin(brainPhase * 4) * 0.2})`;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.arc(rx, ry, 30 + Math.sin(brainPhase * 2) * 4, 0, Math.PI * 2);
        ctx.stroke();
      }
    }

    // Core inner glow rings (static, not rotating)
    if (!isOffline && !isSleeping) {
      ctx.strokeStyle = `rgba(112,231,255,${0.1 + glowMix * 0.15})`;
      ctx.lineWidth = 1;
      for (let i = 1; i <= 3; i++) {
        ctx.beginPath();
        ctx.arc(cx, cy, base * (0.1 + i * 0.07), 0, Math.PI * 2);
        ctx.stroke();
      }
    }

    // Nodes
    nodeCache.forEach((n) => {
      const isActiveZone = activeZones.has(n.zone);
      const node = nodePosition(n);
      n.activity = isActiveZone ? Math.min(1, n.activity + 0.03) : n.activity * 0.98;
      const hue = n.zone === "CORE" ? "225,244,255" : "112,231,255";
      ctx.fillStyle = `rgba(${hue},${isActiveZone ? 0.4 + n.activity * 0.5 : 0.15 + n.activity * 0.3})`;
      ctx.beginPath();
      ctx.arc(node[0], node[1], n.rad, 0, Math.PI * 2);
      ctx.fill();
      if (n.zone === "CORE") {
        ctx.strokeStyle = `rgba(225,244,255,${0.5 + n.activity * 0.5})`;
        ctx.lineWidth = 1.4;
        ctx.beginPath();
        ctx.arc(node[0], node[1], 5 + Math.sin(brainPhase * 2) * 1.5, 0, Math.PI * 2);
        ctx.stroke();
      }
    });

    // Central core fill
    const vg = ctx.createRadialGradient(cx, cy, 0, cx, cy, base * 0.2);
    vg.addColorStop(0, `rgba(112,231,255,${0.05 + glowMix * 0.06})`);
    vg.addColorStop(1, "rgba(112,231,255,0)");
    ctx.fillStyle = vg;
    ctx.beginPath();
    ctx.arc(cx, cy, base * 0.2, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = `rgba(225,244,255,${0.55 + glowMix * 0.3})`;
    ctx.beginPath();
    ctx.arc(cx, cy, 2.6, 0, Math.PI * 2);
    ctx.fill();
  }

  function loop() {
    const base = BRAIN.profile[store.brain] || BRAIN.profile.IDLE;
    const profile = {
      ...base,
      intensity: Math.max(base.intensity, store.activity),
      glow: Math.max(base.glow, store.activity * 0.6),
    };
    if (!prefersReducedMotion) {
      spawnSignals(profile, 1 / 60);
    }
    drawBrain(profile);
    rafId = requestAnimationFrame(loop);
  }

  /* ---------- Brain state machine ---------- */
  function setBrain(state) {
    const p = BRAIN.profile[state];
    if (!p) return;
    if (store.brain === state) {
      refreshBrain(p);
      return;
    }
    store.brain = state;
    store.zone = p.zone;
    refreshBrain(p);
    renderIntentBand();
  }

  function refreshBrain(profile) {
    const stateEl = $("#brain-state"), detailEl = $("#brain-detail");
    const zoneEl = $("#brain-zone"), actEl = $("#activity-value");
    const sigEl = $("#signal-density"), focusEl = $("#context-focus");
    const footEl = $("#foot-state"), liveEl = $("#live-pulse");
    if (stateEl) {
      stateEl.textContent = store.brain;
      const classes = ["warn", "err", "lock"];
      classListToggle(stateEl, classes, false);
      if (store.brain === "WARNING") stateEl.classList.add("warn");
      if (store.brain === "ERROR") stateEl.classList.add("err");
      if (store.brain === "SECURITY_LOCK") stateEl.classList.add("lock");
    }
    if (detailEl) detailEl.textContent = profile.detail;
    if (zoneEl) zoneEl.textContent = profile.zone;
    if (actEl) actEl.textContent = Math.round(Math.max(profile.intensity, store.activity) * 100) + "%";
    if (sigEl) sigEl.textContent = Math.round(profile.density * 100) + "%";
    if (focusEl) focusEl.textContent = profile.focus;
    if (footEl) {
      footEl.textContent = store.brain;
      footEl.classList.toggle("warn", store.brain === "WARNING");
      footEl.classList.toggle("err", store.brain === "ERROR");
      footEl.classList.toggle("lock", store.brain === "SECURITY_LOCK");
    }
    if (liveEl) liveEl.textContent = "LIVE ZUSTAND · " + store.brain;
  }

  function decayToIdle(delay) {
    const token = ++store.decayToken;
    setTimeout(() => {
      if (store.decayToken === token && store.brain !== "IDLE") {
        setBrain("IDLE");
      }
    }, delay);
  }

  /* ---------- UiIntent (orchestrator) ----------
   * The backend projects real JAMES state into a declarative intent
   * (`ui.intent` events on the bus). The renderer follows the orchestrator
   * instead of guessing from raw event names; the EVENT_TO_BRAIN fallback only
   * drives the fabric when no orchestra exists (e.g. static preview). */

  const INTENT_TO_BRAIN = {
    idle: "IDLE", listening: "LISTENING", understanding: "UNDERSTANDING", thinking: "THINKING",
    planning: "PLANNING", searching: "SEARCHING", executing: "EXECUTING", verifying: "VERIFYING",
    completed: "COMPLETED", waiting: "WAITING", learning: "LEARNING", warning: "WARNING",
    error: "ERROR", recovering: "RECOVERING", resource_limited: "RESOURCE_LIMITED",
    security_lock: "SECURITY_LOCK", sleeping: "SLEEPING", offline: "OFFLINE",
  };

  function applyIntent(intent) {
    if (!intent || typeof intent !== "object") return false;
    const state = INTENT_TO_BRAIN[String(intent.brain_state || "").toLowerCase()];
    if (!state) return false;
    store.intent = intent;
    store.mode = String(intent.mode || "JETZT").toUpperCase();
    store.showPanels = Array.isArray(intent.show) ? intent.show : [];
    store.openPanels = Array.isArray(intent.open) ? intent.open : [];
    store.actions = Array.isArray(intent.actions) ? intent.actions : [];
    if (typeof intent.activity === "number") store.activity = clamp(intent.activity, 0, 1);
    if (state !== "SECURITY_LOCK") store.securityDecision = "";
    ++store.decayToken; // decay is orchestrator-driven now
    setBrain(state);
    const focusEl = $("#context-focus");
    if (focusEl && intent.focus) focusEl.textContent = String(intent.focus).toUpperCase();
    const modeEl = $("#mode-label");
    if (modeEl) modeEl.textContent = "VOID / " + store.mode;
    updateSecuritySeal();
    renderIntentBand();
    return true;
  }

  function updateSecuritySeal() {
    const seal = $("#security-seal");
    if (!seal) return;
    if (store.brain === "SECURITY_LOCK") {
      seal.classList.add("lock");
    } else {
      seal.classList.remove("lock", "ask", "deny");
    }
    const dec = store.securityDecision;
    seal.classList.toggle("ask", dec === "ASK");
    seal.classList.toggle("deny", dec === "DENY");
    const lbl = $("#security-label");
    if (lbl && (store.brain === "SECURITY_LOCK" || dec)) {
      lbl.textContent = dec || "PRÜFUNG";
    }
  }

  function captureSecurityDecision(ev) {
    const pl = ev.payload || ev.detail || {};
    const dec = pl && typeof pl === "object" ? (pl.decision || pl.result || pl.outcome || pl.policy || "") : "";
    if (/^(ALLOW|DENY|ASK)$/i.test(String(dec))) {
      store.securityDecision = String(dec).toUpperCase();
    }
  }

  /* ---------- Intent band (adaptive presentation) ----------
   * Mirrors the orchestrator's declarative `show`/`open`/`actions` onto the
   * Stuerzentrale row. The renderer only presents; all decisions are JAMES-side. */

  const SECTION_LABELS = {
    brain: "GEHIRN", progress: "FORTSCHRITT", tasks: "TASKS", sources: "QUELLEN",
    findings: "ERGEBNISSE", errors: "STÖRUNGEN", security: "SECURITY",
    memory: "MEMORY", plan: "PLAN",
  };
  const SECTION_TAB = { tasks: "tasks", sources: "events", findings: "events", errors: "events", security: "security", memory: "memory", progress: "overview" };
  const ACTION_LABELS = {
    inspect_error: "STÖRUNG PRÜFEN", review_security: "SECURITY-ENTSCHEID", resume_task: "AUFGABE FORTSETZEN",
    pause_task: "PAUSE", open_source: "QUELLE ÖFFNEN",
  };
  const SECTION_FALLBACK = ["brain"];

  function fallbackSectionsFor(brain) {
    if (brain === "EXECUTING" || brain === "VERIFYING") return ["brain", "progress", "tasks"];
    if (brain === "PLANNING") return ["brain", "plan", "tasks"];
    if (brain === "SEARCHING") return ["brain", "progress", "sources"];
    if (brain === "LEARNING") return ["brain", "memory"];
    if (brain === "WARNING" || brain === "ERROR" || brain === "RECOVERING") return ["brain", "errors"];
    if (brain === "SECURITY_LOCK") return ["brain", "security"];
    return SECTION_FALLBACK;
  }

  function fallbackIntentActions(brain) {
    if (brain === "WARNING" || brain === "ERROR" || brain === "RECOVERING") return ["inspect_error"];
    if (brain === "SECURITY_LOCK") return ["review_security"];
    if (brain === "WAITING") return ["resume_task"];
    return [];
  }

  function renderIntentBand() {
    const band = $("#intent-band");
    if (!band) return;
    const modeEl = $("#intent-mode"), focusEl = $("#intent-focus");
    if (modeEl) modeEl.textContent = "VOID / " + store.mode;
    if (focusEl) focusEl.textContent = "FOKUS " + (store.intent && store.intent.focus ? String(store.intent.focus).toUpperCase() : (BRAIN.profile[store.brain] || {}).focus || "BREIT");

    const sections = store.intent ? (store.showPanels && store.showPanels.length ? store.showPanels : SECTION_FALLBACK) : fallbackSectionsFor(store.brain);
    const sec = $("#intent-sections");
    if (sec) {
      sec.innerHTML = sections.map((s) =>
        `<button class="section-chip here" data-intent-section="${esc(s)}">${esc(SECTION_LABELS[s] || s)}</button>`).join("");
    }

    const actions = (store.actions && store.actions.length ? store.actions : fallbackIntentActions(store.brain));
    const act = $("#intent-actions");
    if (act) {
      act.innerHTML = actions.map((a) =>
        `<button class="action-chip" data-intent-action="${esc(a)}"><span aria-hidden="true">›</span> ${esc(ACTION_LABELS[a] || String(a).replace(/_/g, " "))}</button>`).join("");
    }
  }

  function runIntentAction(action) {
    switch (action) {
      case "inspect_error":
        renderErrorFeed();
        const rail = $("#error-rail");
        if (rail && store.errors.length) {
          rail.hidden = false;
          rail.scrollIntoView({ behavior: "smooth", block: "nearest" });
        } else {
          toast("Störungen", "Aktuell sind keine Störungen erfasst.");
        }
        break;
      case "review_security":
        openDashboard();
        activateTab("security");
        break;
      case "resume_task":
        sendVoidAction("resume_task", "task.resume", {});
        break;
      case "pause_task":
        sendVoidAction("pause_task", "task.pause", {});
        break;
      case "open_source":
        toast("Quellen", "Quellenansicht öffnet sich mit dem nächsten Forschungsintent.");
        break;
      default:
        toast("Aktion", String(action).replace(/_/g, " "));
    }
  }

  function sendVoidAction(action, capability, input) {
    if (!voidWs || voidWs.readyState !== WebSocket.OPEN) {
      toast("Aktion", "Verbindung nicht bereit");
      return;
    }
    voidWs.send(JSON.stringify({
      type: "void.action",
      action,
      capability,
      input,
      caller: "void",
    }));
  }

  function classListToggle(el, classes, force) {
    if (!el) return;
    classes.forEach((c) => el.classList.toggle(c, force));
  }

  /* ---------- Events ---------- */
  function fmtEventValue(v) {
    if (v == null) return "";
    try {
      if (typeof v === "string") return v;
      const s = JSON.stringify(v);
      return s && s.length > 160 ? s.slice(0, 157) + "…" : s;
    } catch (_) {
      return String(v);
    }
  }

  function resolveSeverity(ev) {
    const raw = String(ev.severity || "").toLowerCase();
    if (raw.includes("sec")) return { cls: "sec", label: "SECURITY" };
    if (raw.includes("error")) return { cls: "error", label: "ERROR" };
    if (raw.includes("warn")) return { cls: "warn", label: "WARNUNG" };
    if (raw.includes("debug")) return { cls: "debug", label: "DEBUG" };
    if (raw.includes("trace")) return { cls: "trace", label: "TRACE" };
    return { cls: "info", label: "INFO" };
  }

  function handleEvent(rawEv) {
    let wrap = rawEv;
    try {
      if (typeof wrap === "string") wrap = JSON.parse(wrap);
    } catch (_) { return; }
    const ev = (wrap && wrap.event) || wrap;
    if (!ev || !ev.event_type) return;

    const sev = resolveSeverity(ev);
    ev._severity = sev;
    store.events.unshift(ev);
    if (store.events.length > 400) store.events.length = 400;

    const type = String(ev.event_type).toLowerCase();

    // Orchestrator-driven state: the intent already codes mode, focus, brain
    // state, activity and actions. Render it and stop (no heuristic override,
    // no decay — the orchestrator owns idle).
    if (type === "ui.intent") {
      if (applyIntent(ev.payload)) {
        updateEventRate();
        renderEventsPanel();
        rebuildContext();
      }
      return;
    }

    if (type === "void.action_result") {
      const out = ev.payload?.output;
      toast("Ergebnis", out ? JSON.stringify(out).slice(0, 200) : "OK", "ok");
      return;
    }
    if (type === "void.action_error") {
      const err = ev.payload?.error || "unbekannter Fehler";
      toast("Fehler", err, "err");
      return;
    }

    if (type.includes("security") || type.includes("approval") || type.includes("permission")) {
      captureSecurityDecision(ev);
      updateSecuritySeal();
    }

    if (type.includes("error") || sev.cls === "error") {
      store.errors.push({ t: fmtTime(now()), sev: sev.label, type: ev.event_type, detail: fmtEventValue(ev.payload || ev.detail || "") });
      if (store.errors.length > 12) store.errors.shift();
      renderErrorFeed();
      if (store.settings.notifications) toast(sev.label, ev.event_type + (ev._body ? " · " + ev._body : ""));
    } else if (sev.cls === "warn" || type.includes("warn")) {
      if (store.settings.notifications) toast("Warnung", ev.event_type + (ev._body ? " · " + ev._body : ""));
    }

    let next = null;
    for (const [keys, st] of EVENT_TO_BRAIN) {
      if (keys.some((k) => type.includes(k))) { next = st; break; }
    }
    if (next) {
      setBrain(next);
      const delay = next === "ERROR" ? 16000 : next === "WARNING" ? 12000 : next === "RECOVERING" ? 6000 : 7000;
      decayToIdle(delay);
    }

    renderEventsPanel();
    updateEventRate();
    rebuildContext();
    if (store.lastTab === "events") {
      const dsh = $("#dashboard-view");
      if (dsh && !dsh.hidden) renderDash("events");
    }
  }

  function updateEventRate() {
    const s = now();
    const windowStart = s.getTime() - 10000;
    const cur = store.events.filter((e) => new Date(e.timestamp).getTime() >= windowStart).length;
    const el = $("#event-rate");
    if (el) el.textContent = cur + "/10s";
  }

  /* ---------- Connection ---------- */
  let ws = null, voidWs = null, retryDelay = 2000, voidRetryDelay = 2000;

  function connectWs() {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    let url;
    try { url = proto + "//" + location.host + "/ws/v1/events"; } catch (_) { return; }
    try { ws = new WebSocket(url); } catch (_) { scheduleReconnect(); return; }
    ws.onopen = () => {
      store.connected = true;
      retryDelay = 2000;
      const dot = $("#conn-dot"), lbl = $("#connection-label");
      if (dot) { dot.classList.add("online"); dot.classList.remove("broken"); }
      if (lbl) lbl.textContent = "VERBINDUNG OK";
      toast("System", "Ereignisverbindung hergestellt", "ok");
    };
    ws.onmessage = (m) => {
      try { handleEvent(JSON.parse(m.data)); } catch (_) { }
    };
    ws.onclose = () => {
      store.connected = false;
      const dot = $("#conn-dot"), lbl = $("#connection-label");
      if (dot) { dot.classList.remove("online"); dot.classList.add("broken"); }
      if (lbl) lbl.textContent = "VERBINDUNG UNTERBROCHEN";
      if (store.intent) {
        ++store.decayToken;
        setBrain("OFFLINE");
        updateSecuritySeal();
      }
      scheduleReconnect();
    };
    ws.onerror = () => { try { ws.close(); } catch (_) { } };
  }

  function connectVoidWs() {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    let url;
    try { url = proto + "//" + location.host + "/ws/v1/void"; } catch (_) { return; }
    try { voidWs = new WebSocket(url); } catch (_) { scheduleVoidReconnect(); return; }
    voidWs.onopen = () => {
      voidRetryDelay = 2000;
      console.debug("[void] 2-way socket connected");
    };
    voidWs.onmessage = (m) => {
      try { handleEvent(JSON.parse(m.data)); } catch (_) { }
    };
    voidWs.onclose = () => {
      console.debug("[void] 2-way socket closed");
      scheduleVoidReconnect();
    };
    voidWs.onerror = () => { try { voidWs.close(); } catch (_) { } };
  }

  function scheduleReconnect() {
    setTimeout(connectWs, retryDelay);
    retryDelay = Math.min(retryDelay * 1.6, 15000);
  }

  function scheduleVoidReconnect() {
    setTimeout(connectVoidWs, voidRetryDelay);
    voidRetryDelay = Math.min(voidRetryDelay * 1.6, 15000);
  }

  /* ---------- API ---------- */
  async function apiGet(path, ms) {
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), ms || 6000);
    try {
      const res = await fetch(path, { signal: ctl.signal });
      if (!res.ok) throw new Error("HTTP " + res.status);
      return await res.json();
    } finally {
      clearTimeout(timer);
    }
  }

  async function refreshHero() {
    try {
      const s = await apiGet("/api/v1/data/status");
      const val = (k, d) => (s && s[k] != null ? s[k] : d);
      const upd = (id, txt) => { const el = $(id); if (el) el.textContent = txt; };
      upd("#memory-count", val("memory_entries", "--"));
      upd("#task-count", (val("tasks_pending", 0) + val("tasks_running", 0)) || "--");
      upd("#capability-count", val("capabilities", "--"));
      upd("#event-count", val("events_observed", 0) + " EREIGNISSE");
      const sec = $("#security-label");
      if (sec && store.brain !== "SECURITY_LOCK" && !store.securityDecision) sec.textContent = val("preview_unauthenticated", true) ? "LOOPBACK · PREVIEW" : "BOUNDED";
      const seal = $("#security-seal");
      if (seal) seal.classList.toggle("guarded", val("preview_unauthenticated", true) !== true);
      const up = $("#uptime");
      if (up) up.textContent = "UPTIME " + fmtDuration(val("uptime_secs", (now() - store.startedAt) / 1000));
    } catch (_) { }
    updateSecuritySeal();
    rebuildContext();
  }

  function fmtDuration(secs) {
    secs = Math.max(0, Math.floor(Number(secs) || 0));
    const h = Math.floor(secs / 3600), m = Math.floor((secs % 3600) / 60);
    return h + "H " + String(m).padStart(2, "0") + "M";
  }

  /* ---------- Context relevance engine ---------- */
  function buildContextCards() {
    const cards = [];
    const profile = BRAIN.profile[store.brain] || BRAIN.profile.IDLE;
    const recent = store.events.slice(0, 8);
    const seen = {};

    const add = (priority, title, desc, meta, bar, opts = {}) => {
      cards.push({
        priority,
        title,
        desc,
        meta,
        bar,
        position: opts.position || "FLOATING",
        size: opts.size || "MEDIUM",
        duration: opts.duration || "TEMPORARY",
        intentDriven: opts.intentDriven || false,
      });
    };

    const nonIdle = store.brain !== "IDLE";
    add(nonIdle ? "IMPORTANT" : "BACKGROUND", "Aktueller Zustand", profile.detail,
      "ZONE " + profile.zone + " · FOKUS " + profile.focus, Math.round(profile.intensity * 100),
      { position: "CENTER", size: "FOCUS", duration: "PERSISTENT", intentDriven: true });

    const warns = recent.filter((e) => e._severity && (e._severity.cls === "warn" || e._severity.cls === "error"));
    if (warns.length) {
      add("URGENT", warns.length === 1 ? "Eine Störung erfasst" : warns.length + " Störungen erfasst",
        warns[0].event_type.replace(/_/g, " "), "ZUERST PRÜFEN · " + fmtTime(new Date(warns[0].timestamp || now())), 100,
        { position: "TOP", size: "LARGE", duration: "UNTIL_RESOLVED" });
    }

    if (store.errors.length) {
      const err = store.errors[store.errors.length - 1];
      add("CRITICAL", "Letzter Fehler", err.type.replace(/_/g, " "), err.time + " · " + err.sev, 100,
        { position: "TOP", size: "LARGE", duration: "UNTIL_RESOLVED" });
    }

    const tasksActivity = recent.filter((e) => String(e.event_type).toLowerCase().includes("task")).slice(0, 3);
    if (tasksActivity.length) {
      add("IMPORTANT", "Task-Fluss", tasksActivity.map((e) => e.event_type.replace(/_/g, " ")).join(", "), String(tasksActivity.length) + " EREIGNISSE", 70,
        { position: "RIGHT", size: "MEDIUM", duration: "TEMPORARY" });
    }

    const memWrites = recent.filter((e) => String(e.event_type).toLowerCase().includes("mem")).length;
    if (memWrites) {
      add("NORMAL", "Gedächtnis-Aktivität", memWrites + " Speichervorgänge in der letzten Zeit", "KONSOLIDIERUNG LAUFEND", 60,
        { position: "LEFT", size: "MEDIUM", duration: "TEMPORARY" });
    }

    let secure = true;
    recent.forEach((e) => {
      if (String(e.event_type).toLowerCase().includes("sec")) {
        seen.warn = seen.warn || e;
        if (e._severity && e._severity.cls === "error") secure = false;
      }
    });
    add(secure ? "NORMAL" : "URGENT", "Security-Posture", secure ? "Kein auffälliges Verhalten erkannt." : "Sicherheitsereignis im Protokoll.",
      store.connected ? "LOOPBACK · ECHTZEIT" : "KEINE VERBINDUNG", secure ? 15 : 90,
      { position: secure ? "BOTTOM" : "TOP", size: secure ? "SMALL" : "LARGE", duration: secure ? "TEMPORARY" : "UNTIL_RESOLVED" });

    const activeSrc = new Map();
    recent.forEach((e) => {
      const src = String(e.source || "system");
      if (!(src in activeSrc)) { activeSrc[src] = 0; }
      activeSrc[src]++;
    });
    const topSrc = Object.entries(activeSrc).sort((a, b) => b[1] - a[1])[0];
    if (topSrc) {
      add("NORMAL", "Quelle " + topSrc[0], topSrc[1] + " Ereignisse in der letzten Zeit", "AKTIVSTE QUELLE", clamp(topSrc[1] * 18, 10, 90),
        { position: "LEFT", size: "SMALL", duration: "TEMPORARY" });
    }

    if (cards.length < 3) {
      add("BACKGROUND", "System bereit", "JAMES wartet auf Aufgaben und Eingaben.", "DEINSTANZ WINDOWS", 5,
        { position: "FLOATING", size: "MICRO", duration: "MOMENTARY" });
    }
    return cards.slice(0, 5);
  }

  function rebuildContext() {
    const layer = $("#context-layer");
    if (!layer) return;
    const cards = buildContextCards();
    const order = { CRITICAL: 0, URGENT: 1, IMPORTANT: 2, NORMAL: 3, BACKGROUND: 4 };
    cards.sort((a, b) => order[a.priority] - order[b.priority]);
    layer.innerHTML = cards.map((c, i) => `
      <div class="context-card priority-${c.priority.toLowerCase()} pos-${c.position.toLowerCase()} size-${c.size.toLowerCase()} dur-${c.duration.toLowerCase()}" style="animation-delay:${i * 40}ms">
        <div class="cc-top">
          <span>${c.priority === "CRITICAL" ? "PRIORITÄT · KRITISCH" : c.priority === "URGENT" ? "PRIORITÄT · DRINGEND" : c.priority === "IMPORTANT" ? "PRIORITÄT · WICHTIG" : c.priority === "NORMAL" ? "PRIORITÄT · NORMAL" : "HINTERGRUND"}</span>
          <span class="cc-tense"><i class="tick"></i></span>
        </div>
        <h4>${esc(c.title)}</h4>
        <p>${esc(c.desc)}</p>
        <div class="cc-meta">${esc(c.meta)}</div>
        <div class="cc-bar"><i style="width:${clamp(c.bar, 2, 100)}%"></i></div>
      </div>`).join("");
    const focus = $("#context-focus");
    if (focus) focus.textContent = (store.intent && store.intent.focus ?
      String(store.intent.focus).toUpperCase() : (BRAIN.profile[store.brain] || {}).focus || "BREIT");
  }

  /* ---------- Error feed ---------- */
  function renderErrorFeed() {
    const feed = $("#error-feed"), rail = $("#error-rail");
    if (!feed) return;
    if (!store.errors.length) { if (rail) rail.hidden = true; return; }
    if (rail) rail.hidden = false;
    feed.innerHTML = store.errors.map((e) => `
      <div class="error-item">
        <span class="ei-time">${e.t}</span>
        <span class="ei-sev">${esc(e.sev)}</span>
        <span>${esc(e.type)}${e.detail ? " — " + esc(e.detail) : ""}</span>
      </div>`).join("");
  }

  /* ---------- Chat ---------- */
  function persistMessages() {
    if (store.settings.persistChat) localStorage.setItem("james.chat", JSON.stringify(store.messages.slice(-60)));
    else localStorage.removeItem("james.chat");
  }

  function chatNode(text, role) {
    const wrap = document.createElement("div");
    wrap.className = "msg " + (role === "user" ? "msg-user" : "msg-bot");
    const meta = document.createElement("span");
    meta.className = "msg-meta";
    const stamp = store.settings.timestamps ? " · " + fmtTime(now()) : "";
    meta.textContent = (role === "user" ? "DU" : "JAMES") + stamp;
    wrap.appendChild(meta);
    const body = document.createElement("div");
    body.innerHTML = formatMessage(text);
    wrap.appendChild(body);
    return wrap;
  }

  function formatMessage(text) {
    let out = esc(String(text || ""))
      .replace(/&lt;task:([^&]+)&gt;([^&]*)&lt;\/task&gt;/g, '<span class="task-chip">✓ $1 — $2</span>');
    const split = out.split("\n");
    const grouped = [];
    let para = [];
    split.forEach((line) => {
      const t = line.trim();
      if (!t) { if (para.length) { grouped.push(para.join("\n")); para = []; } return; }
      para.push(line);
    });
    if (para.length) grouped.push(para.join("\n"));
    return grouped.map((block) => "<p>" + block
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/\n/g, "<br>") + "</p>").join("");
  }

  function appendMessage(text, role, status) {
    if (role === "user") {
      store.messages.push({ role, text: String(text), t: now().toISOString() });
      persistMessages();
    }
    const box = $("#chat-history");
    if (box) {
      const node = chatNode(text, role);
      if (status) {
        const st = document.createElement("div");
        st.className = "msg-status";
        st.innerHTML = '<span class="typing"><i></i><i></i><i></i></span> ' + esc(status);
        node.appendChild(st);
      }
      box.appendChild(node);
      box.scrollTop = box.scrollHeight;
    }
    return box;
  }

  function renderSavedMessages() {
    const box = $("#chat-history");
    if (!box) return;
    box.innerHTML = "";
    store.messages.forEach((m) => {
      if (m.role !== "user" && m.role !== "bot") return;
      box.appendChild(chatNode(m.text || "", m.role));
    });
    box.scrollTop = box.scrollHeight;
  }

  function setTyping(active, label) {
    const st = $("#chat-status");
    if (!st) return;
    st.innerHTML = active
      ? '<span class="typing"><i></i><i></i><i></i></span> ' + esc(label || "JAMES denkt nach …")
      : "";
  }

  async function sendChat(message) {
    const text = String(message || "").trim();
    if (!text) return;
    appendMessage(text, "user");
    const input = $("#chat-input");
    if (input) input.value = "";
    const cmd = text.split(" ")[0].toLowerCase();
    if (["/help", "/h"].includes(cmd)) {
      appendMessage("Befehle:\n\n- **/memory** — Speicherlage anzeigen\n- **/status** — Systemlage anzeigen\n- **/clear** — Chat leeren\n\nAlle anderen Eingaben werden an JAMES' Kern gerichtet.", "bot");
      return;
    }
    if (["/clear", "/c"].includes(cmd)) {
      store.messages = [];
      persistMessages();
      const box = $("#chat-history");
      if (box) box.innerHTML = "";
      return;
    }
    if (cmd === "/memory") {
      setTyping(true, "Speicherlage wird abgefragt …");
      try {
        const data = await apiGet("/api/v1/data/memory");
        appendMemoryReply(data);
      } catch (_) {
        appendMessage("Die Speicherlage ist gerade nicht erreichbar.", "bot");
      } finally {
        setTyping(false);
      }
      return;
    }
    if (cmd === "/status") {
      setTyping(true, "Systemlage wird abgefragt …");
      try {
        const data = await apiGet("/api/v1/data/status");
        const lines = [
          "**Kern:** " + (data.core_status || "unbekannt"),
          "**Module:** " + (data.modules || 0) + " aktiv",
          "**Capabilities:** " + (data.capabilities || 0),
          "**Memory:** " + (data.memory_entries || 0) + " Einträge",
          "**Tasks:** " + (data.tasks_active || 0) + " aktiv · " + (data.tasks_completed || 0) + " erledigt",
          "**Uptime:** " + fmtDuration(data.uptime_secs || 0),
        ];
        appendMessage(lines.join("\n"), "bot");
      } catch (_) {
        appendMessage("Die Systemlage ist gerade nicht erreichbar.", "bot");
      } finally {
        setTyping(false);
      }
      return;
    }
    setTyping(true);
    try {
      const res = await fetch("/api/v1/chat", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message: text }),
      });
      if (!res.ok) throw new Error("HTTP " + res.status);
      const data = await res.json();
      const reply = String(data.content || data.response || "");
      store.messages.push({ role: "bot", text: reply, t: now().toISOString() });
      persistMessages();
      appendMessage(reply, "bot");
      setBrain("THINKING");
      decayToIdle(6000);
    } catch (_) {
      setTyping(false);
      appendMessage("Entschuldigung — der Kern ist nicht erreichbar. Ist JAMES gestartet?", "bot");
    } finally {
      setTyping(false);
    }
  }

  function appendMemoryReply(data) {
    if (!data || !data.total) {
      appendMessage("Der Speicher ist noch leer. Er wird sich mit jedem Gespräch und jeder Aufgabe füllen.", "bot");
      return;
    }
    const entries = Array.isArray(data.entries) ? data.entries : [];
    const lines = [
      "**Speicherlage:** " + data.total + " Einträge",
      "**Arbeitsspeicher:** " + (data.working != null ? data.working : "nicht einzeln gemeldet"),
      "**Konsolidiert:** " + (data.consolidated != null ? data.consolidated : "noch nicht verdichtet"),
    ];
    if (entries.length) {
      lines.push("\nLetzte Einträge:");
      entries.slice(0, 5).forEach((e) => lines.push("- " + (e.content || "…")));
    }
    appendMessage(lines.join("\n"), "bot");
  }

  /* ---------- Dashboard ---------- */
  const TAB_SECTION = {
    overview: "status",
    chat: null,
    tasks: "tasks",
    memory: "memory",
    modules: "modules",
    ai: "ai",
    agents: "agents",
    devices: "devices",
    capabilities: "capabilities",
    events: "events",
    automation: "automation",
    system: "resources",
    security: "security",
    settings: "settings",
  };

  function activateTab(tab) {
    store.lastTab = tab;
    localStorage.setItem("james.tab", tab);
    $$(".tab", $("#dash-nav")).forEach((b) => b.classList.toggle("active", b.dataset.tab === tab));
    const title = $$(".tab").find((b) => b.dataset.tab === tab);
    const h = $("#dash-title");
    if (h && title) h.textContent = title.textContent;
    renderDash(tab);
  }

  function openDashboard() {
    const v = $("#void-view"), d = $("#dashboard-view");
    if (v) v.hidden = true;
    if (d) { d.hidden = false; d.classList.add("active-view"); }
    activateTab(store.lastTab || "overview");
  }

  function closeDashboard() {
    const v = $("#void-view"), d = $("#dashboard-view");
    if (v) v.hidden = false;
    if (d) { d.hidden = true; d.classList.remove("active-view"); }
  }

  const sectionRenders = {
    overview: renderOverview,
    chat: renderChatTab,
    tasks: renderTasksTab,
    memory: renderMemoryTab,
    modules: renderModulesTab,
    ai: renderAITab,
    agents: renderAgentsTab,
    devices: renderDevicesTab,
    capabilities: renderCapabilitiesTab,
    events: renderEventsTab,
    automation: renderAutomationTab,
    system: renderSystemTab,
    security: renderSecurityTab,
    settings: renderSettingsTab,
  };

  async function renderDash(tab) {
    const content = $("#dash-content");
    if (!content) return;
    content.innerHTML = '<div class="empty"><b>Lade …</b></div>';
    const render = typeof sectionRenders[tab] === "function" ? sectionRenders[tab] : renderOverview;
    try {
      const section = TAB_SECTION[tab] || "status";
      const data = tab === "events" ? null : await apiGet("/api/v1/data/" + section);
      content.innerHTML = render(data, tab);
      content.focus({ preventScroll: true });
      window._dashData = data;
    } catch (e) {
      content.innerHTML = renderHonestError(e);
    }
  }

  function renderHonestError(e) {
    return `<div class="empty"><b>Datenquelle nicht erreichbar</b>${esc(e && e.message ? e.message : "")}<br><br>Der Backend-Dienst antwortet gerade nicht. Die Ansicht ist ehrlich leer.</div>`;
  }

  function statCard(label, value, sub, valueClass) {
    return `<div class="card stat"><span class="stat-label">${esc(label)}</span><span class="stat-value ${valueClass || "gold"}">${esc(value)}</span><span class="stat-sub">${esc(sub || "")}</span></div>`;
  }

  function renderOverview(data) {
    const d = data || {};
    const num = (k, alt) => (d[k] != null ? d[k] : alt);
    const grid = [
      statCard("Kern", num("core_status", "—"), "Betriebszustand", "green"),
      statCard("Module", num("modules", "—"), "geladen & aktiv"),
      statCard("Capabilities", num("capabilities", "—"), "registrierte Fähigkeiten"),
      statCard("Memory", num("memory_entries", "—"), "gespeicherte Einträge"),
      statCard("Tasks aktiv", (num("tasks_pending", 0) + num("tasks_running", 0)) || "—", num("tasks_pending", 0) + " wartend · " + num("tasks_running", 0) + " laufend"),
      statCard("Tasks erledigt", num("tasks_completed", "—"), "insgesamt abgeschlossen"),
      statCard("Uptime", fmtDuration(num("uptime_secs", 0)), "seit Systemstart"),
      statCard("Ereignisse", num("events_observed", "—"), "am Bus beobachtet"),
    ];
    return `
      <div class="dash-grid" style="--i:0">${grid.join("")}</div>
      <div class="card">
        <div class="card-title"><span>SYSTEMLAGE</span><b>${esc(d.core_status || "—")}</b></div>
        <div class="kv">
          <dt>Version</dt><dd><b>${esc(d.version || "—")}</b></dd>
          <dt>Preview-Zugang</dt><dd>${d.preview_unauthenticated ? "loopback · unauthentifiziert" : "gesichert"}</dd>
          <dt>Fehler am Bus</dt><dd>${esc(d.events_dropped != null ? d.events_dropped : "—")}</dd>
        </div>
      </div>`;
  }

  function renderChatTab() {
    const count = store.messages.length;
    return `
      <div class="dash-grid" style="--i:0">
        ${statCard("Nachrichten", count, "in dieser Sitzung", "white")}
        ${statCard("Geteilte Tasks", store.messages.filter((m) => String(m.text || "").includes("<task:")).length, "erkannte Marker")}
        ${statCard("Verbindung", store.connected ? "ONLINE" : "OFFLINE", "Ereignis-WS", store.connected ? "green" : "red")}
        ${statCard("Brain", store.brain, "aktueller Zustand")}
      </div>
      <div class="card">
        <div class="card-title"><span>DIALOGVERLAUF</span></div>
        <div class="list-row"><span class="lr-main"><span class="lr-name">Letzte Eingaben</span><br><span class="lr-sub">im Chat-Dock unten</span></span><span class="lr-val">${count} Einträge</span></div>
      </div>`;
  }

  function renderTasksTab(data) {
    const d = data || {};
    const list = Array.isArray(d.tasks) ? d.tasks : [];
    const stats = [
      statCard("Gesamt", d.total != null ? d.total : list.length, "zugeordnete Tasks"),
      statCard("Aktiv", d.active != null ? d.active : list.filter((t) => /running|queued/i.test(String(t.status || ""))).length, "laufend / wartend"),
      statCard("Kritisch", d.critical != null ? d.critical : list.filter((t) => /critical/i.test(String(t.priority || ""))).length, "höchste Priorität"),
      statCard("Erfolg", d.successful != null ? d.successful : list.filter((t) => /completed/i.test(String(t.status || ""))).length, "abgeschlossen"),
    ];
    const body = list.length
      ? `<table class="table"><thead><tr><th>Task</th><th>Status</th><th>Priorität</th><th>Ziel</th></tr></thead><tbody>
          ${list.slice(0, 30).map((t) => `<tr><td><b>${esc(t.name || t.id || "—")}</b></td><td>${pill(String(t.status || ""))}</td><td>${esc(t.priority || "—")}</td><td>${esc(t.capability || "—")}</td></tr>`).join("")}
        </tbody></table>`
      : `<div class="empty"><b>Keine Tasks vorhanden</b>Aktuelle Aufgaben erscheinen hier, sobald JAMES welche annimmt.</div>`;
    return `<div class="dash-grid" style="--i:0">${stats.join("")}</div><div class="card"><div class="card-title"><span>TASKLISTE</span><b>${list.length}</b></div>${body}</div>`;
  }

  function pill(status) {
    const s = String(status || "").toLowerCase();
    const out = (c) => `<span class="pill ${c}">${esc(status)}</span>`;
    if (s.includes("complete") || s.includes("done") || s.includes("success")) return out("ok");
    if (s.includes("fail") || s.includes("error") || s.includes("cancel") || s.includes("cancelled")) return out("err");
    if (s.includes("run") || s.includes("active") || s.includes("queued")) return out("active-pill");
    if (s.includes("paus") || s.includes("wait")) return out("warn");
    return `<span class="pill">${esc(status || "—")}</span>`;
  }

  function renderMemoryTab(data) {
    const d = data || {};
    const total = d.total != null ? d.total : 0;
    const working = d.working != null ? d.working : "—";
    const consolidated = d.consolidated != null ? d.consolidated : "—";
    const recent = Array.isArray(d.entries) ? d.entries : [];
    const max = Math.max(1, total || 1);
    const body = recent.length
      ? `<table class="table"><thead><tr><th>Schlüssel</th><th>Typ</th><th>Wichtigkeit</th><th>Zeit</th></tr></thead><tbody>
          ${recent.slice(0, 30).map((e) => `<tr><td><b>${esc(e.content || "—")}</b></td><td>${esc(String(e.memory_type || "—").replace("MemoryType::", ""))}</td><td>${esc(e.importance != null ? e.importance : "—")}</td><td>${esc(fmtTime(new Date(e.created_at || now())))}</td></tr>`).join("")}
        </tbody></table>`
      : `<div class="empty"><b>Speicher ist leer</b>Mit jedem Gespräch und jeder Aufgabe wächst das Langzeitgedächtnis.</div>`;
    return `
      <div class="dash-grid" style="--i:0">
        ${statCard("Einträge", total, "insgesamt", "gold")}
        ${statCard("Arbeitsspeicher", working, "kurzfristig aktiv")}
        ${statCard("Konsolidiert", consolidated, "langfristig verdichtet")}
      </div>
      <div class="card">
        <div class="card-title"><span>SPEICHERFÜLLUNG</span><b>${total}</b></div>
        <div class="bar-track"><div class="bar-fill" style="width:${clamp((total / 4000) * 100, 1, 100)}%"></div></div>
      </div>
      <div class="card" style="margin-top:14px"><div class="card-title"><span>LETZTE EINTRÄGE</span><b>${recent.length}</b></div>${body}</div>`;
  }

  function renderModulesTab(data) {
    const d = data || {};
    const arr = Array.isArray(d.modules) ? d.modules : [];
    const running = arr.filter((m) => String(m.status || "idle").toLowerCase().includes("run") || (m.is_running === true)).length;
    const total = d.total != null ? d.total : arr.length;
    const body = arr.length
      ? `<table class="table"><thead><tr><th>Modul</th><th>Kategorie</th><th>Status</th></tr></thead><tbody>
          ${arr.map((m) => `<tr><td><b>${esc(m.name || m.id || "—")}</b></td><td>${esc(m.category || "—")}</td><td>${String(m.is_running || m.running) === "true" || String(m.status || "").toLowerCase().includes("run") ? '<span class="pill ok">Aktiv</span>' : '<span class="pill">Bereit</span>'}</td></tr>`).join("")}
        </tbody></table>`
      : `<div class="empty"><b>Keine Moduldaten gemeldet</b>Das Backend hat keine Modulliste geliefert.</div>`;
    return `<div class="dash-grid" style="--i:0">
        ${statCard("Module", total, "im System")}
        ${statCard("Gestartet", running, "aktiv laufend", "green")}
        ${statCard("Kataloge", d.categories != null ? d.categories : "—", "Modulgruppen")}
      </div>
      <div class="card"><div class="card-title"><span>MODULMATRIX</span><b>${arr.length}</b></div>${body}</div>`;
  }

  function renderAITab(data) {
    const d = data || {};
    const raw = Array.isArray(d.models) ? d.models : [];
    const list = raw.map((m) => typeof m === "string" ? { model: m } : m);
    const router = d.router_running ? "aktiv" : d.running ? "bereit" : "passiv";
    const body = list.length
      ? `<table class="table"><thead><tr><th>Provider</th><th>Modell</th><th>Status</th></tr></thead><tbody>
          ${list.map((m) => `<tr><td><b>${esc(m.provider || "lokal")}</b></td><td>${esc(m.model || m.id || "—")}</td><td>${String(m.ready || d.running) === "true" ? '<span class="pill ok">Bereit</span>' : '<span class="pill">…</span>'}</td></tr>`).join("")}
        </tbody></table>`
      : `<div class="empty"><b>Keine Modelle registriert</b>Modelle erscheinen, sobald ein KI-Provider verdrahtet ist (register_provider).</div>`;
    return `<div class="dash-grid" style="--i:0">
        ${statCard("Modelle", list.length, "registriert")}
        ${statCard("Router", router, "Modellauswahl", "white")}
        ${statCard("Politik", d.provider_note || "lokal", "Provider")}
      </div>
      <div class="card"><div class="card-title"><span>KI-PROVIDER</span></div>${body}</div>`;
  }

  function renderAgentsTab(data) {
    const d = data || {};
    const agents = Array.isArray(d.agents) ? d.agents : [];
    return `<div class="dash-grid" style="--i:0">
        ${statCard("Agenten", d.total != null ? d.total : agents.length, "instanziiert")}
        ${statCard("Aktiv", d.active != null ? d.active : agents.filter((a) => a.active).length, "im Einsatz", "green")}
      </div>
      <div class="card"><div class="card-title"><span>AGENTEN</span></div>
      ${agents.length ? `<div class="list-row"><span class="lr-main"><span class="lr-name">${esc(agents[0].name || "—")}</span><br><span class="lr-sub">${esc(agents[0].role || "")}</span></span><span class="lr-val">${esc(agents[0].state || "…")}</span></div>` : `<div class="empty"><b>Agentenräume sind unentdeckt</b>${esc(d.runtime || "Spezialisierte Agenten sind ein geplanter Ausbau (Task F1).")}</div>`}
      </div>`;
  }

  function renderDevicesTab(data) {
    const d = data || {};
    const devices = Array.isArray(d.devices) ? d.devices : [];
    return `<div class="dash-grid" style="--i:0">
        ${statCard("Geräte", d.total != null ? d.total : devices.length, "erkannt")}
        ${statCard("Aktiv", d.active != null ? d.active : devices.filter((x) => x.active).length, "verbunden", "green")}
      </div>
      <div class="card"><div class="card-title"><span>GERÄTE &amp; PERIPHERIE</span></div>
      ${devices.length ? devices.map((g) => `<div class="list-row"><span class="lr-main"><span class="lr-name">${esc(g.name || "—")}</span><br><span class="lr-sub">${esc(g.kind || "")}</span></span><span class="lr-val">${esc(g.state || "bereit")}</span></div>`).join("")
        : `<div class="empty"><b>Geräte-Layer ist rein virtuell</b>Audio- und Browserkanäle sind verdrahtet, aber noch nicht an physische Hardware gebunden (Task F1).</div>`}
      </div>`;
  }

  function renderCapabilitiesTab(data) {
    const d = data || {};
    const caps = Array.isArray(d.capabilities) ? d.capabilities : [];
    const ready = d.ready != null ? d.ready : caps.filter((c) => c.available).length;
    const groups = {};
    caps.forEach((c) => { const k = c.category || "sonstiges"; (groups[k] = groups[k] || []).push(c); });
    const body = caps.length
      ? Object.entries(groups).map(([cat, arr]) => `<div class="card-title" style="margin-top:14px"><span>${esc(cat.toUpperCase())}</span><b>${arr.length}</b></div>
          <table class="table"><thead><tr><th>Fähigkeit</th><th>Provider</th><th>Status</th></tr></thead><tbody>
          ${arr.slice(0, 20).map((c) => `<tr><td><b>${esc(c.name || c.id || "—")}</b></td><td>${esc(c.provider || "—")}</td><td>${c.available !== false ? '<span class="pill ok">Bereit</span>' : '<span class="pill warn">Gesperrt</span>'}</td></tr>`).join("")}
          </tbody></table>`).join("")
      : `<div class="empty"><b>Keine Fähigkeiten registriert</b>Das Backend meldet den Capability-Katalog separat.</div>`;
    return `<div class="dash-grid" style="--i:0">
        ${statCard("Fähigkeiten", d.total != null ? d.total : caps.length, "registriert")}
        ${statCard("Bereit", ready, "verfügbar", "green")}
        ${statCard("Nutzung", d.usages != null ? d.usages : "—", "Aufrufe")}
      </div>
      <div class="card"><div class="card-title"><span>CAPABILITY-KATALOG</span><b>${caps.length}</b></div>${body}</div>`;
  }

  function renderEventsTab() {
    const rows = store.events.slice(0, 80);
    const body = rows.length
      ? `<table class="table"><thead><tr><th>Zeit</th><th>Typ</th><th>Quelle</th><th>Stufe</th></tr></thead><tbody>
          ${rows.map((e) => `<tr><td>${esc(fmtTime(new Date(e.timestamp || now())))}</td><td><b>${esc(e.event_type)}</b></td><td>${esc(e.source || "—")}</td><td>${pillSeverity(e)}</td></tr>`).join("")}
        </tbody></table>`
      : `<div class="empty"><b>Noch keine Ereignisse</b>Live-Ereignisse vom JAMES-Bus erscheinen hier in Echtzeit.</div>`;
    return `<div class="dash-grid" style="--i:0">
        ${statCard("Beobachtet", store.events.length, "seit Verbindung")}
        ${statCard("Verbindung", store.connected ? "ONLINE" : "OFFLINE", "WebSocket", store.connected ? "green" : "red")}
        ${statCard("Störungen", store.errors.length, "erfasst")}
      </div>
      <div class="card"><div class="card-title"><span>LIVE-EREIGNISSE</span><b>${store.events.length}</b></div>${body}</div>`;
  }

  function pillSeverity(e) {
    const sev = (e._severity || resolveSeverity(e));
    const map = { error: "err", warn: "warn", ok: "ok" };
    const cls = map[sev.cls] || "";
    return `<span class="pill ${cls}">${esc(sev.label)}</span>`;
  }

  function renderAutomationTab(data) {
    const d = data || {};
    const rules = Array.isArray(d.automations) ? d.automations : [];
    const state = d.runtime || "workflow engine is not implemented yet";
    const active = rules.length > 0;
    return `
      <div class="card">
        <div class="card-title"><span>AUTOMATIONS-MASCHINERIE</span><b>${active ? "AKTIV" : "AUS"}</b></div>
        <div class="kv">
          <dt>Runtime</dt><dd>${esc(state)}</dd>
          <dt>Automationen</dt><dd>${rules.length} definiert</dd>
        </div>
        ${rules.length ? rules.map((r) => `<div class="list-row"><span class="lr-main"><span class="lr-name">${esc(r.name || "—")}</span><br><span class="lr-sub">${esc(r.trigger || r.when || "")}</span></span><span class="lr-val">${pill(r.state || r.status || "idle")}</span></div>`).join("")
          : `<div class="empty"><b>Offene P/Task F1</b>Die Automatisierungs-Engine ist dokumentiert, aber noch nicht in Betrieb genommen.</div>`}
      </div>`;
  }

  function renderSystemTab(data) {
    const d = data || {};
    const num = (k, alt) => (d[k] != null ? d[k] : alt);
    return `
      <div class="dash-grid" style="--i:0">
        ${statCard("Uptime", fmtDuration(num("uptime_secs", 0)), "Betriebszeit")}
        ${statCard("Ereignisse", num("events_observed", "—"), "am Bus")}
        ${statCard("Verworfen", num("events_dropped", 0), "Bus-Abfall", num("events_dropped", 0) ? "red" : "green")}
        ${statCard("Memory", num("memory_entries", "—"), "Langzeit")}
        ${statCard("Tasks aktiv", num("tasks_active", "—"), "in Arbeit")}
        ${statCard("Instanz", d.instance || "JAMES", "Chassis", "white")}
      </div>
      <div class="card">
        <div class="card-title"><span>RESSOURCEN</span></div>
        <div class="kv">
          <dt>Metriken</dt><dd>${esc(d.platform_metrics || "nicht gemeldet (F1-08)")}</dd>
          <dt>Sammlung</dt><dd>${esc(d.collection || "—")} · ${d.sampled_at ? esc(fmtTime(new Date(d.sampled_at))) : "—"}</dd>
        </div>
      </div>`;
  }

  function renderSecurityTab(data) {
    const d = data || {};
    const posture = d.boundary || "loopback-only";
    const trail = Array.isArray(d.trail) ? d.trail : [];
    const guarded = trail.some((t) => String(t.severity || "").toLowerCase().includes("error")) || trail.length > 30;
    const vals = [
      { k: "Grenze", v: esc(d.boundary || "loopback-only") },
      { k: "Authentifizierung", v: esc(d.authentication || "preview pending") },
      { k: "Vermittler", v: esc(d.broker || "permission · policy · execution") },
      { k: "Protokollspuren", v: trail.length + " Einträge" },
    ];
    return `
      <div class="card" style="${guarded ? `border-color:rgba(192,122,104,.45)` : `border-color:rgba(162,197,140,.35)`}">
        <div class="card-title"><span>SECURITY-POSTURE</span><b style="color:${guarded ? "var(--red)" : "var(--green)"}">${guarded ? "ERHÖHTE LAGE" : "RUHE"}</b></div>
        ${vals.map((v) => `<div class="list-row"><span class="lr-main"><span class="lr-name">${v.k}</span></span><span class="lr-val">${v.v}</span></div>`).join("")}
      </div>
      <div class="card" style="margin-top:14px"><div class="card-title"><span>PRÜFSPUR (CAPABILITY · APPROVAL · AUDIT)</span><b>${trail.length}</b></div>
      ${trail.length ? `<table class="table"><thead><tr><th>Ereignis</th><th>Quelle</th><th>Stufe</th></tr></thead><tbody>
        ${trail.slice(0, 40).map((t) => `<tr><td><b>${esc(t.event_type)}</b></td><td>${esc(t.source || "—")}</td><td>${pillSeverity({ _severity: resolveSeverity(t) })}</td></tr>`).join("")}
      </tbody></table>` : `<div class="empty"><b>Noch keine Prüfspuren</b>Capability-, Approval- und Audit-Ereignisse erscheinen hier.</div>`}
      </div>`;
  }

  function renderSettingsTab() {
    const s = store.settings;
    const group = (id, label, desc) => `
      <div class="list-row">
        <span class="lr-main"><span class="lr-name">${esc(label)}</span><br><span class="lr-sub">${esc(desc)}</span></span>
        <button class="toggle ${s[id] ? "on" : ""}" data-setting="${id}" role="switch" aria-checked="${s[id]}" aria-label="${esc(label)}"></button>
      </div>`;
    return `
      <div class="card">
        <div class="card-title"><span>OBERFLÄCHE</span></div>
        ${group("showDetails", "Detailansichten", "Zusätzliche Metadaten in Karten und Protokollen anzeigen.")}
        ${group("notifications", "Benachrichtigungen", "Toasts bei Warnungen und Fehlern einblenden.")}
        ${group("timestamps", "Zeitstempel", "Uhrzeiten in Chat- und Ereigniszeilen anzeigen.")}
        <div class="list-row">
          <span class="lr-main"><span class="lr-name">Chatverlauf</span><br><span class="lr-sub">${s.persistChat ? "wird lokal gespeichert" : "nur in dieser Sitzung"}</span></span>
          <button class="toggle ${s.persistChat ? "on" : ""}" data-setting="persistChat" role="switch" aria-checked="${s.persistChat}" aria-label="Chatverlauf"></button>
        </div>
      </div>
      <div class="card" style="margin-top:14px">
        <div class="card-title"><span>SYSTEM</span></div>
        <div class="kv">
          <dt>Instanz</dt><dd>JAMES · Void-UI</dd>
          <dt>Transport</dt><dd>Basis · HTTP + WebSocket</dd>
          <dt>Laufzeit</dt><dd>${fmtDuration((now() - store.startedAt) / 1000)} (UI)</dd>
        </div>
      </div>`;
  }

  function bindSettingToggles(scope) {
    const root = scope || document;
    $$(".toggle[data-setting]", root).forEach((t) => {
      t.onclick = () => {
        const key = t.dataset.setting;
        store.settings[key] = !store.settings[key];
        localStorage.setItem("james.settings", JSON.stringify(store.settings));
        t.classList.toggle("on", store.settings[key]);
        t.setAttribute("aria-checked", store.settings[key]);
      };
    });
  }

  /* ---------- Events panel ---------- */
  function renderEventsPanel() {
    const list = $("#event-list");
    if (!list) return;
    const rows = store.events.slice(0, 60);
    list.innerHTML = rows.length
      ? rows.map((e) => {
          const sev = e._severity || resolveSeverity(e);
          const body = fmtEventValue(e.payload || e.detail || "");
          return `<div class="event-row sev-${sev.cls}">
            <span class="er-time">${esc(fmtTime(new Date(e.timestamp || now())))}</span>
            <span class="er-body">
              <span class="er-sev">${esc(sev.label)}</span> · <span class="er-detail" style="display:inline">${esc(e.event_type)}</span>
              <div class="er-src">${esc(e.source || "system")}</div>
              ${body ? `<div class="er-detail">${esc(body)}</div>` : ""}
            </span>
          </div>`;
        }).join("")
      : `<div class="empty"><b>Noch keine Ereignisse</b>Live-Ereignisse erscheinen hier über den Bus.</div>`;
  }

  /* ---------- Toasts ---------- */
  function toast(title, msg, severity) {
    const box = $("#toasts");
    if (!box) return;
    const el = document.createElement("div");
    el.className = "toast " + (severity || "info");
    el.innerHTML = `<div class="toast-title">${esc(title)}</div><div class="toast-msg">${esc(msg)}</div>`;
    box.appendChild(el);
    setTimeout(() => { el.classList.add("out"); setTimeout(() => el.remove(), 300); }, 5200);
    while (box.children.length > 4) box.firstChild.remove();
  }

  /* ---------- Drawer ---------- */
  function openDrawer(kicker, html) {
    const drawer = $("#drawer");
    if (!drawer) return;
    const kick = $("#drawer-kicker"), content = $("#drawer-content");
    if (kick) kick.textContent = kicker;
    if (content) content.innerHTML = html;
    drawer.classList.add("open");
    drawer.setAttribute("aria-hidden", "false");
  }

  const drawerRenders = {
    help: () => openDrawer("JAMES / HILFE", `
      <div class="card"><div class="card-title"><span>KOMMANDOS</span></div>
        <div class="list-row"><span class="lr-main"><span class="lr-name">/help</span><br><span class="lr-sub">diesen Überblick zeigen</span></span></div>
        <div class="list-row"><span class="lr-main"><span class="lr-name">/memory</span><br><span class="lr-sub">Speicherlage abfragen</span></span></div>
        <div class="list-row"><span class="lr-main"><span class="lr-name">/status</span><br><span class="lr-sub">Systemlage abfragen</span></span></div>
        <div class="list-row"><span class="lr-main"><span class="lr-name">/clear</span><br><span class="lr-sub">Chat leeren</span></span></div>
      </div>`),
    info: () => openDrawer("JAMES / ÜBER", `
      <div class="card"><div class="card-title"><span>INSTANZ</span></div>
        <div class="kv"><dt>Sitzung</dt><dd>${esc(uid().slice(0, 8))}</dd><dt>Laufzeit UI</dt><dd>${fmtDuration((now() - store.startedAt) / 1000)}</dd><dt>Verbindung</dt><dd>${store.connected ? "online" : "unterbrochen"}</dd></div>
      </div>`),
  };

  /* ---------- Command palette ---------- */
  const PALETTE_ITEMS = [
    { cat: "Ansicht", name: "Zum Void zurückkehren", run: () => { closeDashboard(); closeEvents(); } },
    { cat: "Ansicht", name: "Dashboard öffnen", run: () => { openDashboard(); closePalette(); } },
    { cat: "Dashboard", name: "Tasks", run: () => { openDashboard(); activateTab("tasks"); closePalette(); } },
    { cat: "Dashboard", name: "Memory", run: () => { openDashboard(); activateTab("memory"); closePalette(); } },
    { cat: "Dashboard", name: "Capabilities", run: () => { openDashboard(); activateTab("capabilities"); closePalette(); } },
    { cat: "Dashboard", name: "System", run: () => { openDashboard(); activateTab("system"); closePalette(); } },
    { cat: "Dashboard", name: "Security", run: () => { openDashboard(); activateTab("security"); closePalette(); } },
    { cat: "Dashboard", name: "Ereignisse", run: () => { openDashboard(); activateTab("events"); closePalette(); } },
    { cat: "Dashboard", name: "Automation", run: () => { openDashboard(); activateTab("automation"); closePalette(); } },
    { cat: "System", name: "Ereignisprotokoll", run: () => { openEvents(); closePalette(); } },
    { cat: "System", name: "Einstellungen", run: () => { openSettings(); closePalette(); } },
    { cat: "Chat", name: "/memory", run: () => { focusChat("/memory"); closePalette(); } },
    { cat: "Chat", name: "/status", run: () => { focusChat("/status"); closePalette(); } },
    { cat: "Chat", name: "/clear", run: () => { focusChat("/clear"); closePalette(); } },
    { cat: "Hilfe", name: "Hilfe öffnen", run: () => { drawerRenders.help(); closePalette(); } },
    { cat: "Hilfe", name: "Über JAMES", run: () => { drawerRenders.info(); closePalette(); } },
  ];

  let paletteIndex = 0, paletteFiltered = [];

  function openPalette() {
    const p = $("#palette");
    if (!p) return;
    store.paletteOpen = true;
    p.classList.add("open");
    p.setAttribute("aria-hidden", "false");
    filterPalette("");
    const input = $("#palette-input");
    if (input) { input.value = ""; setTimeout(() => input.focus(), 20); }
  }

  function closePalette() {
    const p = $("#palette");
    if (!p) return;
    store.paletteOpen = false;
    p.classList.remove("open");
    p.setAttribute("aria-hidden", "true");
  }

  function filterPalette(q) {
    const term = q.toLowerCase().trim();
    paletteFiltered = PALETTE_ITEMS.filter((i) => !term || i.name.toLowerCase().includes(term) || i.cat.toLowerCase().includes(term));
    paletteIndex = 0;
    renderPalette();
  }

  function renderPalette() {
    const list = $("#palette-results");
    if (!list) return;
    list.innerHTML = paletteFiltered.length
      ? paletteFiltered.map((i, idx) => `
          <div class="palette-item ${idx === paletteIndex ? "current" : ""}" data-idx="${idx}">
            <span class="pi-cat">${esc(i.cat)}</span><span class="pi-name">${esc(i.name)}</span>
            <span class="pi-done">↵</span>
          </div>`).join("")
      : `<div class="empty" style="border:none"><b>Keine Treffer</b></div>`;
  }

  function runPaletteItem(idx) {
    const item = paletteFiltered[idx];
    if (item) item.run();
  }

  /* ---------- Events panel toggles ---------- */
  function openEvents() {
    const panel = $("#event-panel");
    if (!panel) return;
    renderEventsPanel();
    panel.classList.add("open");
    panel.setAttribute("aria-hidden", "false");
  }

  function closeEvents() {
    const panel = $("#event-panel");
    if (!panel) return;
    panel.classList.remove("open");
    panel.setAttribute("aria-hidden", "true");
  }

  function openSettings() {
    const drawer = $("#drawer");
    if (!drawer) return;
    openDrawer("JAMES / EINSTELLUNGEN", renderSettingsTab());
    bindSettingToggles($("#drawer-content"));
  }

  function toggleDashboard() {
    const v = $("#dashboard-view");
    if (v && !v.hidden) closeDashboard();
    else openDashboard();
  }

  function focusChat(text) {
    closeDashboard();
    const input = $("#chat-input");
    if (!input) return;
    input.value = text || "";
    input.focus();
    $("#void-view").scrollIntoView({ behavior: "smooth", block: "end" });
  }

  /* ---------- Clock & uptime ---------- */
  function startClock() {
    setInterval(() => {
      const d = now();
      const c = $("#clock"), s = $("#session-clock");
      if (c) c.textContent = fmtTime(d);
      if (s) s.textContent = fmtTime(d);
      const up = $("#uptime");
      if (up) up.textContent = "UPTIME " + fmtDuration((now() - store.startedAt) / 1000);
    }, 1000);
  }

  /* ---------- Boot ---------- */
  function bindActions() {
    const map = {
      "toggle-events": openEvents,
      "close-events": closeEvents,
      "toggle-dashboard": toggleDashboard,
      "toggle-settings": openSettings,
      "open-palette": openPalette,
      "close-drawer": () => { const d = $("#drawer"); if (d) { d.classList.remove("open"); d.setAttribute("aria-hidden", "true"); } },
      "clear-errors": () => { store.errors = []; renderErrorFeed(); },
    };
    document.addEventListener("click", (ev) => {
      const intentAct = ev.target.closest("[data-intent-action]");
      if (intentAct) {
        const action = intentAct.dataset.intentAction;
        if (action) runIntentAction(action);
        return;
      }
      const intentSec = ev.target.closest("[data-intent-section]");
      if (intentSec) {
        const s = intentSec.dataset.intentSection;
        if (s && s !== "brain") {
          const tab = SECTION_TAB[s];
          if (tab) { openDashboard(); activateTab(tab); }
        }
        return;
      }
      const el = ev.target.closest("[data-action]");
      if (!el) return;
      const fn = map[el.dataset.action];
      if (fn) fn();
    });
  }

  function bindUI() {
    const form = $("#chat-form");
    if (form) form.addEventListener("submit", (e) => { e.preventDefault(); sendChat($("#chat-input").value); });

    const nav = $("#dash-nav");
    if (nav) nav.addEventListener("click", (e) => {
      const tab = e.target.closest(".tab");
      if (tab) { e.preventDefault(); activateTab(tab.dataset.tab); }
    });

    const dash = $("#dash-content");
    if (dash) {
      dash.addEventListener("click", (e) => {
        const t = e.target.closest("[data-setting]");
        if (!t) return;
        const key = t.dataset.setting;
        store.settings[key] = !store.settings[key];
        localStorage.setItem("james.settings", JSON.stringify(store.settings));
        t.classList.toggle("on", store.settings[key]);
        t.setAttribute("aria-checked", store.settings[key]);
      });
    }

    const palInput = $("#palette-input");
    if (palInput) palInput.addEventListener("input", () => filterPalette(palInput.value));
    const palList = $("#palette-results");
    if (palList) palList.addEventListener("click", (e) => {
      const item = e.target.closest(".palette-item");
      if (item) runPaletteItem(Number(item.dataset.idx));
    });

    document.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); store.paletteOpen ? closePalette() : openPalette(); return; }
      if (e.key === "Escape") {
        if (store.paletteOpen) { closePalette(); return; }
        closeEvents();
        const dr = $("#drawer");
        if (dr && dr.classList.contains("open")) { dr.classList.remove("open"); dr.setAttribute("aria-hidden", "true"); }
        return;
      }
      if (!store.paletteOpen) return;
      if (e.key === "ArrowDown") { e.preventDefault(); paletteIndex = Math.min(paletteFiltered.length - 1, paletteIndex + 1); renderPalette(); }
      else if (e.key === "ArrowUp") { e.preventDefault(); paletteIndex = Math.max(0, paletteIndex - 1); renderPalette(); }
      else if (e.key === "Enter" && paletteFiltered.length) { e.preventDefault(); runPaletteItem(paletteIndex); }
    });
  }

  function applySettings() {
    const body = document.body;
    if (!body) return;
    body.classList.toggle("reduce-motion", store.settings.reduceMotion);
  }

  function startTicker() {
    setInterval(() => {
      if (!store.connected && store.events.length && (now().getTime() - (store.events[0].timestamp ? new Date(store.events[0].timestamp).getTime() : now().getTime())) > 15000) {
        setBrain("WAITING");
      }
    }, 5000);
  }

  function boot() {
    canvas = $("#brain-canvas");
    bindActions();
    bindUI();
    applySettings();
    renderSavedMessages();
    renderEventsPanel();
    renderErrorFeed();
    renderIntentBand();
    startClock();
    if (canvas) { layout(); loop(); }
    window.addEventListener("resize", () => { layout(); });
    connectWs();
    connectVoidWs();
    refreshHero();
    setInterval(refreshHero, 10000);
    startTicker();
    setBrain("IDLE");
    const h = $("#dash-title");
    if (h) h.textContent = "Übersicht";
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();