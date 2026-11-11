// The fan: a half circle of rows of dots around a turntable, like a roundhouse seen from above. Its geometry follows
// crates/look/src/fan.rs, and its colours and motion crates/look/src/mark.rs, so the site and the app draw one mark.
//
// <canvas class="fan" data-signal="working" data-work="open"> draws a mark as wide as the canvas is laid out.
// data-signal is idle, working, setting-up, background or failed. data-work is a pull request's state, or copper for
// the brand. The fan's colour is the work's alone, the light is the agent's, and the turntable turns red when the
// agent stopped short. A chat that needs a look isn't on the mark: the name beside it says so. data-rows overrides how many rows it has, and data-sweep-s how long the light takes to cross it.

const MORE = 4;
const HOLE = [2, 5];
const DOT = 0.32;
const MARGIN = 1.25;
const TURNTABLE = 0.07;
const ASPECT = 0.62;

const LIGHT = 0.92;
// The light's lag from the inner row to the outer, shared out between the rows, as the app icon has it.
const TRAIL = 0.18;
const SWEEP_S = 2.4;
const BUILD_S = 4.8;
// Slower than a working agent's sweep, so something left running isn't taken for the agent.
const BACKGROUND_S = 3.6;
// A still mark is drawn at this moment, as the app's pictures are. A still hero has its light where the icon has it.
const STILL_S = 0.9;
const STILL_SWEEP = 0.66;
// Where a mark's light starts and how far it goes. A big fan's light starts and ends off every row, so it doesn't
// blink out of the outer row as it comes round again, which a mark is too small to show.
const SWEEP = { from: -0.25, span: 1.5 };
const HERO_SWEEP = { from: -0.3, span: 1.8 };

const COLORS = {
  copper: "#d08a5c",
  none: "#6f6964",
  draft: "#8b949e",
  open: "#3fb950",
  merged: "#a371f7",
  closed: "#b9605a",
  red: "#ee6f64",
  light: "#ebe7e3",
};

const rgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const mix = (a, b, t) => a.map((v, i) => Math.round(v + (b[i] - v) * t));

function firstRow(rows) {
  const [part, whole] = HOLE;
  const spaces = Math.ceil((part * MORE * (rows - 1)) / (whole - part));
  return spaces + (spaces % 2) + 1;
}

// The app's rows for its sizes, and beyond them, a row for every 36px or so, so the dots stay as fine.
function rowsFor(width) {
  if (width <= 32) return 4;
  if (width <= 76) return 5;
  if (width <= 120) return 7;
  return Math.max(9, Math.round(1 + width / 36));
}

function layout(width, height, rows) {
  const first = firstRow(rows);
  const spaces = first - 1 + MORE * (rows - 1);
  const outer = (width * 0.5) / (1 + ((1 + MARGIN) * DOT * Math.PI) / spaces);
  const spacing = (outer * Math.PI) / spaces;
  const dot = Math.max(spacing * DOT, 0.45);
  const radius = width * TURNTABLE;
  // What's drawn sits in the middle of the box, top to bottom, from the top of the outer row to the bottom of the
  // turntable, so a fan beside a line of text looks level with it.
  const cx = width / 2;
  const cy = (height + outer + dot - radius) / 2;
  const dots = [];
  for (let row = 0; row < rows; row++) {
    const count = first + MORE * row;
    const r = (spacing * (count - 1)) / Math.PI;
    for (let j = 0; j < count; j++) {
      const along = j / (count - 1);
      const angle = Math.PI * (along - 1);
      dots.push({ x: cx + r * Math.cos(angle), y: cy + r * Math.sin(angle), along, row });
    }
  }
  return {
    cx,
    cy,
    dots,
    dot,
    rows,
    spacing,
    first,
    turntable: { radius, line: Math.max(dot * 0.8, radius * 0.16, 0.6), hub: Math.max(dot, radius * 0.28) },
  };
}

function lightOn(along, row, sweep, trail) {
  return Math.min(1, Math.max(0, 1 - Math.abs(along - (sweep - row * trail)) * 3.5));
}

const phase = (t, period) => (t / period) % 1;

// How strongly a dot shows its colour, and how much white light is on it.
function brightness(mark, dot, t) {
  const trail = TRAIL / (mark.fan.rows - 1);
  switch (mark.signal) {
    case "working": {
      const sweep = mark.moving ? mark.sweep.from + phase(t, mark.sweepS) * mark.sweep.span : mark.stillSweep;
      return [1, lightOn(dot.along, dot.row, sweep, trail)];
    }
    case "background": {
      // Something the agent left running: the light keeps to the inner row, beside the turntable.
      if (dot.row !== 0) return [1, 0];
      const at = mark.moving ? phase(t, BACKGROUND_S) : phase(STILL_S, BACKGROUND_S);
      return [1, lightOn(dot.along, 0, at * 1.5 - 0.25, 0)];
    }
    case "setting-up": {
      const at = mark.moving ? phase(t, BUILD_S) : phase(STILL_S, BUILD_S);
      return dot.along + dot.row * 0.04 < at * 1.25 ? [1, 0.4] : [0.2, 0];
    }
    default:
      return [1, 0];
  }
}

function paint(mark, t) {
  // A mark that isn't laid out, such as one in something hidden, has no size to draw at.
  if (!mark.width) return;
  const { ctx, fan, color } = mark;
  const light = rgb(COLORS.light);
  ctx.clearRect(0, 0, mark.width, mark.height);
  for (const dot of fan.dots) {
    const [base, glow] = brightness(mark, dot, t);
    const [r, g, b] = glow > 0.01 ? mix(color, light, glow * LIGHT) : color;
    ctx.fillStyle = `rgba(${r},${g},${b},${base})`;
    ctx.beginPath();
    ctx.arc(dot.x, dot.y, fan.dot, 0, Math.PI * 2);
    ctx.fill();
  }
  // The turntable is the fan's colour, quieter, unless it has something to say.
  const hub =
    mark.signal === "failed"
      ? COLORS.red
      : mark.signal === "background"
        ? `rgba(${light.join(",")},${LIGHT})`
        : `rgba(${color.join(",")},0.6)`;
  const { radius, line } = fan.turntable;
  ctx.strokeStyle = hub;
  ctx.lineWidth = line;
  ctx.beginPath();
  ctx.arc(fan.cx, fan.cy, radius - line / 2, 0, Math.PI * 2);
  ctx.stroke();
  ctx.fillStyle = hub;
  ctx.beginPath();
  ctx.arc(fan.cx, fan.cy, fan.turntable.hub, 0, Math.PI * 2);
  ctx.fill();
  copyToBloom(mark);
}

function copyToBloom(mark) {
  if (!mark.bloom) return;
  const b = mark.bloom.getContext("2d");
  b.clearRect(0, 0, mark.bloom.width, mark.bloom.height);
  b.drawImage(mark.canvas, 0, 0);
}

const still = matchMedia("(prefers-reduced-motion: reduce)");
const marks = [];
let start = performance.now();

function measure(mark) {
  const { canvas } = mark;
  const width = canvas.clientWidth;
  const height = width * ASPECT;
  const ratio = window.devicePixelRatio || 1;
  canvas.style.height = `${height}px`;
  canvas.width = Math.round(width * ratio);
  canvas.height = Math.round(height * ratio);
  mark.ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  if (mark.bloom) {
    mark.bloom.width = canvas.width;
    mark.bloom.height = canvas.height;
    mark.bloom.style.width = `${width}px`;
    mark.bloom.style.height = `${height}px`;
  }
  mark.width = width;
  mark.height = height;
  mark.fan = layout(width, height, Number(canvas.dataset.rows) || rowsFor(width));
}

function setUp(canvas) {
  const signal = canvas.dataset.signal || "idle";
  const work = canvas.dataset.work || "none";
  const color = rgb(COLORS[work]);
  const bloomId = canvas.dataset.bloom;
  const mark = {
    canvas,
    ctx: canvas.getContext("2d"),
    bloom: bloomId ? document.getElementById(bloomId) : null,
    signal,
    color,
    sweepS: Number(canvas.dataset.sweepS) || SWEEP_S,
    sweep: canvas.dataset.hero === undefined ? SWEEP : HERO_SWEEP,
    stillSweep: canvas.dataset.stillSweep ? Number(canvas.dataset.stillSweep) : phase(STILL_S, SWEEP_S) * 1.5 - 0.25,
    animates: signal === "working" || signal === "setting-up" || signal === "background",
    visible: true,
  };
  mark.hero = canvas.dataset.hero !== undefined;
  if (mark.hero) mark.stillSweep = STILL_SWEEP;
  measure(mark);
  marks.push(mark);
  return mark;
}

// How far through a sweep the light's middle, between its inner and outer rows, is over the middle of the fan.
function middleOf(mark) {
  return (0.5 + TRAIL / 2 - mark.sweep.from) / mark.sweep.span;
}

// The hero's light says when it starts a sweep, and when it passes the middle of the fan, so whatever changes with it
// keeps its time. A still hero says nothing.
function beat(mark, t) {
  const now = phase(t, mark.sweepS);
  const was = mark.lastPhase;
  mark.lastPhase = now;
  if (was === undefined) return;
  const middle = middleOf(mark);
  if (now < was) mark.canvas.dispatchEvent(new CustomEvent("sweep-start"));
  if (was < middle && now >= middle) mark.canvas.dispatchEvent(new CustomEvent("sweep-middle"));
}

function frame(now) {
  const t = (now - start) / 1000;
  for (const mark of marks) {
    mark.moving = mark.animates && !still.matches;
    if (mark.moving && mark.visible) paint(mark, t);
    if (mark.moving && mark.visible && mark.hero) beat(mark, t);
  }
  requestAnimationFrame(frame);
}

function drawAll() {
  const t = (performance.now() - start) / 1000;
  for (const mark of marks) {
    mark.moving = mark.animates && !still.matches;
    paint(mark, mark.moving ? t : STILL_S);
  }
}

document.querySelectorAll("canvas.fan").forEach(setUp);

// The hero's light starts over the middle of the fan, so words that change as it passes the middle each stay for a
// whole sweep, the first included.
const opening = marks.find((mark) => mark.hero);
if (opening) start -= middleOf(opening) * opening.sweepS * 1000;

const seen = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    const mark = marks.find((m) => m.canvas === entry.target);
    if (mark) mark.visible = entry.isIntersecting;
  }
});
marks.forEach((mark) => seen.observe(mark.canvas));

let resizing;
addEventListener("resize", () => {
  cancelAnimationFrame(resizing);
  resizing = requestAnimationFrame(() => {
    marks.forEach(measure);
    drawAll();
  });
});
still.addEventListener("change", drawAll);

drawAll();
requestAnimationFrame(frame);
