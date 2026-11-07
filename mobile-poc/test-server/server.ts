// Simulated remote hub for testing a client on a phone. Serves synthetic sessions by range with
// artificial latency, streams deltas and terminal output, and collects results.
import { readFileSync, readdirSync, mkdirSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";

const HERE = dirname(new URL(import.meta.url).pathname);
export const FIXTURES = join(HERE, "out");
export const RAW = join(HERE, "out", "results");
export const MAX_WINDOW = 500;

type Session = { meta: any; buf: Buffer; offsets: Float64Array };
const sessions = new Map<string, Session>();

function loadSessions() {
  const dir = join(FIXTURES, "sessions");
  if (!existsSync(dir)) throw new Error("Run `bun mobile-poc/test-server/gen.ts` first");
  for (const f of readdirSync(dir).filter((x) => x.endsWith(".jsonl"))) {
    const id = f.replace(".jsonl", "");
    const buf = readFileSync(join(dir, f));
    const meta = JSON.parse(readFileSync(join(dir, `${id}.meta.json`), "utf8"));
    const offsets = new Float64Array(meta.total_items + 1);
    let n = 1, at = 0;
    while (n <= meta.total_items) { at = buf.indexOf(10, at) + 1; offsets[n++] = at; }
    sessions.set(id, { meta, buf, offsets });
  }
}

function rng(seed: number) {
  let a = seed >>> 0;
  return () => { a = (a + 0x6d2b79f5) >>> 0; let t = a; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}
const STREAM_WORDS = "the daemon keeps a window of rows around the anchor so scrolling stays stable while ranges load and `session_id` maps to one branch **always** ".split(" ");
function* streamText(seed: number) {
  const r = rng(seed);
  let block = 0;
  for (;;) {
    block++;
    if (block % 5 === 0) { yield "\n\n```rust\n"; for (let i = 0; i < 12; i++) yield `let value_${i} = compute(${Math.floor(r() * 999)}).await?;\n`; yield "```\n\n"; }
    else if (block % 7 === 0) { yield "\n\n| column | value | status |\n| --- | --- | --- |\n"; for (let i = 0; i < 4; i++) yield `| row${i} | ${Math.floor(r() * 99)} | ok |\n`; yield "\n"; }
    else if (block % 3 === 0) { yield `\n\n## Step ${block}\n\n`; }
    else for (let i = 0; i < 40; i++) yield STREAM_WORDS[Math.floor(r() * STREAM_WORDS.length)] + " ";
  }
}
function terminalChunk(size: number, r: () => number) {
  const parts: string[] = [];
  let n = 0;
  while (n < size) {
    const c = 31 + Math.floor(r() * 6);
    const roll = r();
    const line = roll < 0.9 ? `\x1b[${c}m${"building crate ".padEnd(16)}\x1b[0m ${Math.floor(r() * 1e9).toString(36).repeat(4)} \x1b[1mok\x1b[0m\r\n`
      : roll < 0.97 ? `\x1b[2K\r\x1b[33mprogress ${Math.floor(r() * 100)}%\x1b[0m\r\n`
      : `\x1b[A\x1b[2K\x1b[42;30m PASS \x1b[0m ${"x".repeat(60)}\r\n`;
    parts.push(line); n += line.length;
  }
  return Buffer.from(parts.join(""));
}

export type Run = { run_id: string; contender: string; scenario: string | null; session?: string; spawned_at_ms?: number; params?: any };
export type Handlers = { onMark?: (name: string, body: any) => void; onResult?: (body: any) => void };

export function startServer(port = 7777, handlers: Handlers = {}) {
  loadSessions();
  let run: Run = { run_id: "interactive", contender: "interactive", scenario: null };
  let latency = 100;
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const cors = { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "*", "Access-Control-Allow-Methods": "*" };
  const json = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { "Content-Type": "application/json", ...cors } });

  const server = Bun.serve<{ kind: string; timers: any[] }>({
    port, hostname: "127.0.0.1", idleTimeout: 0,
    async fetch(req, srv) {
      const url = new URL(req.url);
      const p = url.pathname;
      if (req.method === "OPTIONS") return new Response(null, { headers: cors });
      if (p === "/ws/stream" || p === "/ws/terminal") {
        return srv.upgrade(req, { data: { kind: p, timers: [] } }) ? undefined : new Response("upgrade failed", { status: 400 });
      }
      if (p === "/run") return json(run);
      if (p === "/workspaces") return new Response(readFileSync(join(FIXTURES, "workspaces.json")), { headers: { "Content-Type": "application/json", ...cors } });
      if (p === "/sessions") { await sleep(latency); return json([...sessions.values()].map((s) => ({ id: s.meta.id, title: s.meta.title, total_items: s.meta.total_items }))); }
      const m = p.match(/^\/sessions\/([\w-]+)\/items$/);
      if (m) {
        const s = sessions.get(m[1]);
        if (!s) return json({ error: "no such session" }, 404);
        const total = s.meta.total_items;
        const start = Math.max(0, Number(url.searchParams.get("start") ?? 0));
        const end = Math.min(total, Number(url.searchParams.get("end") ?? start + 100));
        if (end - start > MAX_WINDOW) return json({ error: `window exceeds ${MAX_WINDOW}` }, 400);
        await sleep(latency);
        const lines = s.buf.subarray(s.offsets[start], s.offsets[end]).toString("utf8").trimEnd().split("\n").filter(Boolean);
        return new Response(`{"start":${start},"end":${end},"total":${total},"items":[${lines.join(",")}]}`, { headers: { "Content-Type": "application/json", ...cors } });
      }
      if (p.startsWith("/diffs/")) { await sleep(latency); return new Response(readFileSync(join(FIXTURES, `${p.slice(7)}.patch`)), { headers: { "Content-Type": "text/plain", ...cors } }); }
      if (p === "/mark" && req.method === "POST") { const b: any = await req.json(); handlers.onMark?.(b.name, b); return json({ ok: true }); }
      if (p === "/results" && req.method === "POST") {
        const b: any = await req.json();
        const dir = join(RAW, b.run_id ?? run.run_id);
        mkdirSync(dir, { recursive: true });
        writeFileSync(join(dir, `${b.contender}-${b.scenario}.json`), JSON.stringify(b));
        handlers.onResult?.(b);
        return json({ ok: true });
      }
      return json({ error: "not found" }, 404);
    },
    websocket: {
      perMessageDeflate: false,
      message(ws, raw) {
        const msg = JSON.parse(String(raw));
        if (ws.data.kind === "/ws/stream") {
          const { sessions: ids = [], chunks_per_sec = 60, duration_ms = 30000 } = msg;
          const started = Date.now();
          ids.forEach((id: string, idx: number) => {
            const gen = streamText(1000 + idx);
            let seq = 0;
            const t = setInterval(() => {
              if (Date.now() - started > duration_ms) { clearInterval(t); ws.send(JSON.stringify({ session: id, done: true, seq })); return; }
              ws.send(JSON.stringify({ session: id, seq: seq++, text: gen.next().value }));
            }, 1000 / chunks_per_sec);
            ws.data.timers.push(t);
          });
        } else {
          const { bytes_per_sec = 20 * 1048576, duration_ms = 15000 } = msg;
          const r = rng(99), tickMs = 10, per = Math.round((bytes_per_sec * tickMs) / 1000), started = Date.now();
          let sent = 0;
          const t = setInterval(() => {
            if (Date.now() - started > duration_ms) { clearInterval(t); ws.send(JSON.stringify({ done: true, bytes: sent })); return; }
            if (ws.getBufferedAmount() > 64 * 1048576) return; // Flow control, like a PTY blocking its writer.
            const chunk = terminalChunk(per, r);
            sent += chunk.length;
            ws.send(chunk);
          }, tickMs);
          ws.data.timers.push(t);
        }
      },
      close(ws) { ws.data.timers.forEach(clearInterval); },
    },
  });
  return { server, setRun: (r: Run) => { run = r; }, setLatency: (ms: number) => { latency = ms; }, sessionIds: () => [...sessions.keys()], total: (id: string) => sessions.get(id)?.meta.total_items ?? 0, stop: () => server.stop(true) };
}

if (import.meta.main) {
  const s = startServer(Number(process.env.PORT ?? 7777));
  s.setLatency(Number(process.env.LATENCY_MS ?? 100));
  console.log(`test server on http://127.0.0.1:${s.server.port} sessions=${s.sessionIds().join(",")} (interactive mode)`);
}
