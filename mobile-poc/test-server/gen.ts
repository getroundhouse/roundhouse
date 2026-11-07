// Deterministic synthetic fixture generator. Contains no real transcript content.
// Usage: bun mobile-poc/test-server/gen.ts [--out dir] [--only s,m,l,xl]
import { mkdirSync, createWriteStream, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { TIERS, MIX, SEED, GIANT, DIFF } from "./config.ts";

const arg = (name: string, fallback: string) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : fallback;
};
const OUT = arg("out", join(dirname(new URL(import.meta.url).pathname), "out"));
const ONLY = arg("only", Object.keys(TIERS).join(",")).split(",");

function rng(seed: number) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return {
    f: next,
    int: (lo: number, hi: number) => lo + Math.floor(next() * (hi - lo + 1)),
    pick: <T,>(xs: readonly T[]): T => xs[Math.floor(next() * xs.length)],
    chance: (p: number) => next() < p,
    // Log-normal size draw, clamped. Agent output sizes are heavy tailed.
    size: (median: number, sigma: number, max: number) => {
      const u = Math.max(next(), 1e-9), v = next();
      const z = Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
      return Math.min(max, Math.max(1, Math.round(median * Math.exp(sigma * z))));
    },
  };
}
type R = ReturnType<typeof rng>;

const WORDS = "workspace branch session daemon protocol range window cursor buffer stream token layout render scroll anchor viewport segment index cache eviction transcript message handler adapter process socket relay worktree commit rebase merge review comment diff hunk highlight parser grammar scheduler executor runtime channel backpressure snapshot checkpoint migration schema table column query latency throughput frame vsync texture atlas glyph shaping selection clipboard focus keyboard shortcut palette sidebar panel terminal scrollback provider sandbox credential permission approval the a an of to in for with when while because after before this that it we can should must does not only then also each every both between during against through without returns handles updates keeps preserves drops retries fails succeeds".split(" ");
const IDENTS = "session_id workspace_path item_count anchor_offset frame_budget scroll_state range_start range_end byte_offset line_height row_cache event_log turn_id parent_branch pending_ops retry_delay max_window resolved_model tool_name exit_code".split(" ");
const TYPES = "Session Workspace EventLog RangeRequest FrameTimer RowCache ScrollAnchor DiffHunk Highlighter TermGrid".split(" ");
const PATHS = "crates/daemon/src/session.rs crates/daemon/src/worktree.rs crates/protocol/src/range.rs apps/desktop/src/transcript/list.tsx apps/desktop/src/diff/viewer.tsx packages/client/src/store.ts scripts/setup.sh migrations/0042_events.sql".split(" ");
const INTL = ["工作区已归档,稍后可以恢复。", "変更をレビューしてからマージしてください。", "작업 공간을 다시 시작했습니다.", "✅ tests passed 🚀 3 flaky ⚠️ retried", "مساحة العمل جاهزة للمراجعة", "👩‍💻 fixed the race in `flush()` 🎉", "Überprüfung abgeschlossen: naïve café résumé"];

const sentence = (r: R) => {
  const n = r.int(6, 22);
  const w: string[] = [];
  for (let i = 0; i < n; i++) {
    const roll = r.f();
    w.push(roll < 0.07 ? "`" + r.pick(IDENTS) + "`" : roll < 0.09 ? "**" + r.pick(WORDS) + "**" : roll < 0.1 ? `[${r.pick(WORDS)}](https://example.com/${r.pick(WORDS)})` : r.pick(WORDS));
  }
  const s = w.join(" ");
  return s[0].toUpperCase() + s.slice(1) + ".";
};
const para = (r: R, target: number) => {
  let out = "";
  while (out.length < target) out += (out ? " " : "") + sentence(r);
  return out;
};
const codeLine = (r: R, lang: string, indent: string) => {
  const a = r.pick(IDENTS), b = r.pick(IDENTS), T = r.pick(TYPES), n = r.int(0, 4096);
  switch (lang) {
    case "rust": return indent + r.pick([`let ${a}: ${T} = ${b}.get(${n}).cloned().unwrap_or_default();`, `if ${a}.len() > ${n} { return Err(anyhow!("${r.pick(WORDS)} failed: {}", ${b})); }`, `self.${a} = ${T}::new(${b}, "${r.pick(WORDS)}");`, `// ${sentence(r)}`, `for ${a} in ${b}.iter().filter(|x| x.id != ${n}) { tx.send(${a}.clone()).await?; }`]);
    case "ts": return indent + r.pick([`const ${a}: ${T} = await client.${b}({ start: ${n}, end: ${n + 500} });`, `if (!${a}?.ok) throw new Error(\`${r.pick(WORDS)} failed: \${${b}}\`);`, `// ${sentence(r)}`, `export function ${b}(${a}: ${T}): number { return ${a}.items.length + ${n}; }`, `setState((prev) => ({ ...prev, ${a}: prev.${b} ?? ${n} }));`]);
    case "python": return indent + r.pick([`${a} = ${T}(${b}=${n}, name="${r.pick(WORDS)}")`, `if ${a} is None or len(${b}) > ${n}: raise ValueError(f"{${a}} ${r.pick(WORDS)}")`, `# ${sentence(r)}`, `return [x for x in ${b} if x.${a} != ${n}]`]);
    case "bash": return r.pick([`git worktree add ../${r.pick(WORDS)} -b feature/${r.pick(WORDS)}`, `cargo test -p ${r.pick(WORDS)} -- --nocapture`, `curl -sS "$API/${r.pick(WORDS)}?limit=${n}" | jq '.data[].${a}'`, `# ${sentence(r)}`]);
    default: return indent + `"${a}": { "${b}": ${n}, "kind": "${r.pick(WORDS)}", "ok": ${r.chance(0.5)} },`;
  }
};
const code = (r: R, lang: string, lines: number) => {
  const out: string[] = [];
  let depth = 0;
  for (let i = 0; i < lines; i++) {
    if (lang !== "bash" && lang !== "json" && r.chance(0.06)) depth = r.int(0, 3);
    out.push(codeLine(r, lang, "    ".repeat(depth)));
  }
  return out.join("\n");
};
const LANGS = ["rust", "ts", "python", "bash", "json"] as const;
const fence = (r: R, lines: number) => { const l = r.pick(LANGS); return "```" + l + "\n" + code(r, l, lines) + "\n```"; };
const table = (r: R) => {
  const cols = r.int(2, 5), rows = r.int(2, 8);
  const row = () => "| " + Array.from({ length: cols }, () => r.pick(WORDS)).join(" | ") + " |";
  return [row(), "|" + " --- |".repeat(cols), ...Array.from({ length: rows }, row)].join("\n");
};
const markdown = (r: R, target: number) => {
  const parts: string[] = [];
  let size = 0;
  while (size < target) {
    const roll = r.f();
    let p: string;
    if (roll < 0.45) p = para(r, Math.min(target - size + 40, r.int(120, 700)));
    else if (roll < 0.58) p = Array.from({ length: r.int(2, 7) }, () => (r.chance(0.5) ? "- " : "1. ") + sentence(r)).join("\n");
    else if (roll < 0.66) p = "#".repeat(r.int(2, 4)) + " " + r.pick(WORDS) + " " + r.pick(WORDS);
    else if (roll < 0.9) p = fence(r, Math.max(2, Math.min(60, Math.round((target - size) / 45))));
    else if (roll < 0.96) p = table(r);
    else p = "> " + sentence(r) + "\n\n" + r.pick(INTL);
    parts.push(p);
    size += p.length;
  }
  return parts.join("\n\n");
};
const logText = (r: R, target: number) => {
  const lines: string[] = [];
  let size = 0;
  while (size < target) {
    const roll = r.f();
    const line = roll < 0.45 ? `${r.pick(PATHS)}:${r.int(1, 900)}:${r.int(1, 90)}: ${r.pick(["warning", "error", "note"])}: ${sentence(r)}`
      : roll < 0.8 ? codeLine(r, r.pick(LANGS), "")
      : roll < 0.995 ? `test ${r.pick(WORDS)}::${r.pick(WORDS)} ... ${r.pick(["ok", "ok", "ok", "FAILED", "ignored"])}`
      : "x".repeat(r.int(2000, 12000)); // minified line
    lines.push(line);
    size += line.length + 1;
  }
  return lines.join("\n");
};

function makeItem(r: R, i: number, giantAt: Set<number>) {
  const ts = new Date(Date.UTC(2026, 8, 1) + i * 9000).toISOString();
  const id = `it_${i}`;
  if (giantAt.has(i)) {
    const size = r.int(GIANT.min_chars, GIANT.max_chars);
    return { i, id, ts, role: "tool", blocks: [{ type: "tool_result", name: "Bash", is_error: false, expanded: true, text: logText(r, size) }] };
  }
  const roll = r.f();
  if (roll < MIX.user) {
    return { i, id, ts, role: "user", blocks: [{ type: "text", text: r.chance(0.15) ? markdown(r, r.size(180, 0.9, 4600)) : para(r, r.size(90, 0.9, 900)) }] };
  }
  if (roll < MIX.user + MIX.tool_use) {
    const name = r.pick(MIX.tool_names);
    const input = name === "Bash" ? codeLine(r, "bash", "") : name === "Edit" ? JSON.stringify({ file_path: r.pick(PATHS), old_string: code(r, "ts", r.int(1, 8)), new_string: code(r, "ts", r.int(1, 12)) }) : JSON.stringify({ file_path: r.pick(PATHS), limit: r.int(20, 400) });
    return { i, id, ts, role: "assistant", blocks: [{ type: "tool_use", name, input }] };
  }
  if (roll < MIX.user + MIX.tool_use + MIX.tool_result) {
    return { i, id, ts, role: "tool", blocks: [{ type: "tool_result", name: r.pick(MIX.tool_names), is_error: r.chance(0.04), expanded: i % 10 === 0, text: logText(r, r.size(MIX.tool_result_median, 1.5, MIX.tool_result_max)) }] };
  }
  if (roll < MIX.user + MIX.tool_use + MIX.tool_result + MIX.thinking) {
    return { i, id, ts, role: "assistant", blocks: [{ type: "thinking", expanded: false, text: para(r, r.size(MIX.thinking_median, 1.1, 72000)) }] };
  }
  if (roll < MIX.user + MIX.tool_use + MIX.tool_result + MIX.thinking + MIX.image) {
    return { i, id, ts, role: "user", blocks: [{ type: "image", width: r.int(400, 1600), height: r.int(300, 1000) }, { type: "text", text: sentence(r) }] };
  }
  return { i, id, ts, role: "assistant", blocks: [{ type: "text", text: markdown(r, r.size(MIX.text_median, 1.1, MIX.text_max)) }] };
}

async function writeSession(tier: string, items: number) {
  const r = rng(SEED ^ [...tier].reduce((a, c) => a * 31 + c.charCodeAt(0), 7));
  const giantAt = new Set<number>();
  for (let n = 0; n < Math.max(1, Math.round(items / GIANT.one_per)); n++) giantAt.add(r.int(0, items - 1));
  const stream = createWriteStream(join(OUT, "sessions", `${tier}.jsonl`));
  let bytes = 0;
  const kinds: Record<string, number> = {};
  for (let i = 0; i < items; i++) {
    const it = makeItem(r, i, giantAt);
    for (const b of it.blocks) kinds[b.type] = (kinds[b.type] ?? 0) + 1;
    const line = JSON.stringify(it) + "\n";
    bytes += Buffer.byteLength(line);
    if (!stream.write(line)) await new Promise((res) => stream.once("drain", res));
  }
  await new Promise((res) => stream.end(res));
  writeFileSync(join(OUT, "sessions", `${tier}.meta.json`), JSON.stringify({ id: tier, title: `Synthetic ${tier} session`, total_items: items, bytes, kinds, giant_items: [...giantAt].sort((a, b) => a - b) }, null, 1));
  console.log(`${tier}: ${items} items, ${(bytes / 1048576).toFixed(1)} MB`, kinds);
}

function writeDiff() {
  const r = rng(SEED ^ 0xd1ff);
  const out: string[] = [];
  const perFile = Math.ceil(DIFF.lines / DIFF.files);
  let total = 0;
  for (let f = 0; f < DIFF.files && total < DIFF.lines; f++) {
    const lang = f % 2 === 0 ? "rust" : "ts";
    const path = `src/${r.pick(WORDS)}/${r.pick(WORDS)}_${f}.${lang === "rust" ? "rs" : "ts"}`;
    out.push(`diff --git a/${path} b/${path}`, `--- a/${path}`, `+++ b/${path}`);
    total += 3;
    let budget = perFile, at = r.int(1, 200);
    while (budget > 0) {
      const ctx = r.int(2, 6), del = r.int(0, 14), add = r.int(1, 18);
      const size = ctx * 2 + del + add;
      out.push(`@@ -${at},${ctx * 2 + del} +${at},${ctx * 2 + add} @@ fn ${r.pick(IDENTS)}()`);
      for (let i = 0; i < ctx; i++) out.push(" " + codeLine(r, lang, "    "));
      for (let i = 0; i < del; i++) out.push("-" + codeLine(r, lang, "    "));
      for (let i = 0; i < add; i++) out.push("+" + codeLine(r, lang, "    "));
      for (let i = 0; i < ctx; i++) out.push(" " + codeLine(r, lang, "    "));
      at += size + r.int(10, 60);
      budget -= size + 1;
      total += size + 1;
    }
  }
  writeFileSync(join(OUT, "diff-20k.patch"), out.join("\n") + "\n");
  console.log(`diff: ${out.length} lines`);
}

function writeWorkspaces() {
  const r = rng(SEED ^ 0x5eed);
  const repos = ["daemon", "desktop", "protocol", "infra"];
  const statuses = ["running", "running", "idle", "review", "checks-failing", "merged"];
  const ws = Array.from({ length: 24 }, (_, i) => ({ id: `ws_${i}`, repo: r.pick(repos), name: `${r.pick(WORDS)}-${r.pick(WORDS)}`, branch: `feature/${r.pick(WORDS)}-${i}`, status: r.pick(statuses), unread: r.chance(0.3), adds: r.int(0, 2400), dels: r.int(0, 900) }));
  writeFileSync(join(OUT, "workspaces.json"), JSON.stringify(ws, null, 1));
}

mkdirSync(join(OUT, "sessions"), { recursive: true });
for (const [tier, items] of Object.entries(TIERS)) if (ONLY.includes(tier)) await writeSession(tier, items);
writeDiff();
writeWorkspaces();
