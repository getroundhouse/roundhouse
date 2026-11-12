#!/usr/bin/env python3
"""Regenerates crates/agents/tests/fixtures/claude.jsonl, claude_controls.jsonl, claude_background.jsonl,
claude_mcp_task.jsonl and claude_mcp_task_resumed.jsonl from the real `claude` CLI.

Run this after upgrading the CLI, then run `cargo test -p agents`. A failure there means the CLI's
output changed in a way the adapter depends on.

It uses the adapter's exact flags in a scratch repo, with trivial prompts, on a cheap model. Costs a
fraction of a cent. Machine details and personal configuration are removed before anything is
written, because the output goes in the repo.

With --controls, it only regenerates claude_controls.jsonl: control requests and the CLI's replies, with no
prompt, so nothing is spent. With --background, it only regenerates claude_background.jsonl: work the agent leaves
running in the background, which takes a few seconds to finish, and then a few more to be stopped. With --mcp-task, it
only regenerates the two claude_mcp_task files: a call to a tool that's answered with an MCP task, as
`wait_for_pull_request` is, from a stand-in for the worker's tools, the task ending and the CLI picking up by itself,
then another, and the CLI stopped with SIGTERM as a worker stops it. Then, in the second file, the same conversation
resumed, which lists the task again, until it ends. It prints what the CLI asked of the stand-in, so a CLI that calls
off its task as it stops shows.
"""
import http.server
import json
import os
import pathlib
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "crates/agents/tests/fixtures/claude.jsonl"
CONTROLS = ROOT / "crates/agents/tests/fixtures/claude_controls.jsonl"
BACKGROUND = ROOT / "crates/agents/tests/fixtures/claude_background.jsonl"
MCP_TASK = ROOT / "crates/agents/tests/fixtures/claude_mcp_task.jsonl"
MCP_TASK_RESUMED = ROOT / "crates/agents/tests/fixtures/claude_mcp_task_resumed.jsonl"
FLAGS = """-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages
--thinking-display summarized --permission-prompt-tool stdio --allow-dangerously-skip-permissions --permission-mode manual --model haiku --effort high""".split()
# Keeps the fixture small. Order is preserved, so the protocol's sequencing is still real.
MAX_PER_KIND = {"thinking_tokens": 2, "status": 3, "hook_started": 0, "hook_response": 0}


def user(text):
    return {"type": "user", "session_id": "", "parent_tool_use_id": None,
            "message": {"role": "user", "content": [{"type": "text", "text": text}]}}


class Cli:
    def __init__(self, cwd, args=(), env_set=None):
        scrub = ("CLAUDECODE", "CLAUDE_AGENT_SDK_VERSION", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_ENTRYPOINT",
                 "CLAUDE_CODE_EXECPATH", "CLAUDE_CODE_MESSAGING_SOCKET", "CLAUDE_CODE_MESSAGING_TOKEN",
                 "CLAUDE_CODE_SESSION_ATTENDED", "CLAUDE_CODE_SESSION_ID", "CLAUDE_EFFORT", "CLAUDE_PID")
        env = {k: v for k, v in os.environ.items() if k not in scrub}
        env["CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS"] = "1"
        env.update(env_set or {})
        self.proc = subprocess.Popen(["claude", *FLAGS, *args], cwd=cwd, env=env, text=True,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.lines, self.seen = [], []
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        for line in self.proc.stdout:
            value = json.loads(line)
            self.lines.append(value)
            self.seen.append(value)

    def send(self, value):
        self.proc.stdin.write(json.dumps(value) + "\n")
        self.proc.stdin.flush()

    def stop(self, sig):
        """Signals the CLI, and returns once it has exited and everything it wrote on its way out has been read."""
        self.proc.send_signal(sig)
        self.proc.wait(timeout=30)
        self.reader.join(timeout=10)

    def wait_for(self, predicate, timeout=90):
        deadline = time.time() + timeout
        while time.time() < deadline:
            for value in self.seen:
                if predicate(value):
                    self.seen = []
                    return value
            time.sleep(0.05)
        sys.exit("timed out waiting for the CLI")


def is_approval(value):
    return value.get("type") == "control_request" and value["request"].get("subtype") == "can_use_tool"


def is_result(value):
    return value.get("type") == "result"


def respond(cli, request, response):
    cli.send({"type": "control_response", "response": {"subtype": "success", "request_id": request["request_id"], "response": response}})


def capture(cwd):
    cli = Cli(cwd)
    cli.send(user("Think briefly about what 6*7 is. Then use the Bash tool to run exactly: echo 42 > answer.txt   Then reply with the single word DONE."))
    request = cli.wait_for(is_approval)
    respond(cli, request, {"behavior": "allow", "updatedInput": request["request"]["input"]})
    cli.wait_for(is_result)
    time.sleep(0.3)

    cli.send(user("Use the Bash tool to run exactly: echo nope > nope.txt"))
    request = cli.wait_for(is_approval)
    respond(cli, request, {"behavior": "deny", "message": "Not allowed"})
    cli.wait_for(is_result)
    time.sleep(0.3)

    cli.send(user("Use the Bash tool to run exactly: echo again > again.txt"))
    cli.wait_for(is_approval)
    cli.send({"type": "control_request", "request_id": "interrupt-1", "request": {"subtype": "interrupt", "cancel_queued": True}})
    cli.wait_for(is_result)
    time.sleep(0.5)

    # A change it accepts, one it refuses, and leaving plan mode, which it reports by itself.
    cli.send({"type": "control_request", "request_id": "mode-1", "request": {"subtype": "set_permission_mode", "mode": "plan"}})
    cli.wait_for(lambda v: v.get("type") == "control_response" and v["response"].get("request_id") == "mode-1")
    cli.send({"type": "control_request", "request_id": "model-1", "request": {"subtype": "set_model", "model": "not-a-model"}})
    cli.wait_for(lambda v: v.get("type") == "control_response" and v["response"].get("request_id") == "model-1")
    cli.send(user("Make a one line plan to create a file planned.txt containing hello, then call ExitPlanMode. Do not ask questions."))
    request = cli.wait_for(lambda v: is_approval(v) and v["request"].get("tool_name") == "ExitPlanMode")
    respond(cli, request, {"behavior": "allow", "updatedInput": request["request"]["input"]})
    # It goes on to carry out the plan, and that isn't needed here.
    if is_approval(cli.wait_for(lambda v: is_approval(v) or is_result(v))):
        cli.send({"type": "control_request", "request_id": "interrupt-2", "request": {"subtype": "interrupt", "cancel_queued": True}})
        cli.wait_for(is_result)
    time.sleep(0.3)

    # A question, answered the way the app answers one.
    cli.send(user("Use the AskUserQuestion tool to ask me whether I prefer red or blue (one question, two options). Then say which I chose."))
    request = cli.wait_for(is_approval)
    question = request["request"]["input"]["questions"][0]
    answers = {question["question"]: question["options"][1]["label"]}
    respond(cli, request, {"behavior": "allow", "updatedInput": {**request["request"]["input"], "answers": answers}})
    cli.wait_for(is_result)
    time.sleep(0.3)
    cli.proc.kill()
    return cli.lines


def capture_background(cwd):
    """A command the agent leaves running in the background, which finishes once its turn has ended, and the turn the
    agent starts by itself to take that in. Then another, still running when SIGTERM stops the CLI, as the worker
    stops an agent, which stops that work, says so, and exits."""
    cli = Cli(cwd)

    def until_result(answer):
        """Answers each approval asked for, until the turn ends. Some commands it runs without asking."""
        while is_approval(request := cli.wait_for(lambda v: is_approval(v) or is_result(v))):
            respond(cli, request, answer(request))

    def leave_running(command):
        cli.send(user(f"Use the Bash tool with run_in_background set to true to run exactly: {command}   "
                      "Do not wait for it or check on it. Reply with the single word STARTED."))
        until_result(lambda request: {"behavior": "allow", "updatedInput": request["request"]["input"]})

    leave_running("sleep 3; echo finished")
    # The turn it starts by itself, in which it may want to read what the command wrote, which it needn't.
    until_result(lambda request: {"behavior": "deny", "message": "Not needed"})
    time.sleep(0.3)
    leave_running("sleep 300")
    time.sleep(0.5)
    cli.proc.terminate()
    cli.proc.wait(timeout=30)
    time.sleep(0.3)
    return cli.lines


def capture_controls(cwd):
    """A level it takes, one it doesn't, and a new model, each followed by what it then reports applying. Then how
    full its context is, what's left of its account's limits, and a request it doesn't know."""
    cli = Cli(cwd)
    requests = [
        ("settings-1", {"subtype": "get_settings"}),
        ("effort-1", {"subtype": "apply_flag_settings", "settings": {"effortLevel": "low"}}),
        ("settings-2", {"subtype": "get_settings"}),
        ("effort-2", {"subtype": "apply_flag_settings", "settings": {"effortLevel": "ultra"}}),
        ("settings-3", {"subtype": "get_settings"}),
        ("model-1", {"subtype": "set_model", "model": "sonnet"}),
        ("settings-4", {"subtype": "get_settings"}),
        ("context-1", {"subtype": "get_context_usage"}),
        ("usage-1", {"subtype": "get_usage"}),
        ("unknown-1", {"subtype": "not_a_request"}),
    ]
    exchanged = []
    for request_id, request in requests:
        sent = {"type": "control_request", "request_id": request_id, "request": request}
        cli.send(sent)
        reply = cli.wait_for(lambda v: v.get("type") == "control_response" and v["response"].get("request_id") == request_id)
        exchanged += [sent, reply]
    cli.proc.kill()
    return exchanged


class TaskServer:
    """A stand-in for the worker's tools, on MCP's protocol 2026-07-28 over HTTP, as `crates/worker/src/tools/mcp.rs`
    answers it: `wait_for_pull_request` answers each call with a task, which is working until `end` is called, then
    completed, or until `call_off` is, then cancelled, as the hub calls a wait off for the person. It writes down each
    method it's asked, with the task it names."""
    VERSION = "2026-07-28"
    TASKS = "io.modelcontextprotocol/tasks"
    TOOL = "wait_for_pull_request"

    def __init__(self):
        self.asked, self.tasks, self.last_task, self.lock = [], {}, None, threading.Lock()
        server = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
                answer = server.answer(body)
                if answer is None:
                    self.send_response(202)
                    self.end_headers()
                    return
                data = json.dumps(answer).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *_):
                pass

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}/mcp"

    def task(self, task_id):
        status = self.tasks[task_id]
        task = {"resultType": "complete", "taskId": task_id, "status": status, "createdAt": "2100-01-01T00:00:00Z",
                "lastUpdatedAt": "2100-01-01T00:00:00Z", "ttlMs": None, "pollIntervalMs": 500,
                "statusMessage": "Waiting on pull request #7. Its checks are running."}
        if status == "cancelled":
            task["statusMessage"] = "The wait was called off."
        if status == "completed":
            task["statusMessage"] = "The wait ended: it's merged."
            task["result"] = {"resultType": "complete", "isError": False, "content": [
                {"type": "text", "text": "The wait on pull request #7 has ended: it's merged."}]}
        return task

    def answer(self, body):
        method, params = body.get("method"), body.get("params") or {}
        with self.lock:
            self.asked.append((method, params.get("taskId") or params.get("name")))
            if "id" not in body:
                return None
            if method == "server/discover":
                result = {"resultType": "complete", "supportedVersions": [self.VERSION],
                          "capabilities": {"tools": {}, "extensions": {self.TASKS: {}}},
                          "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "roundhouse", "version": "0"}},
                          "ttlMs": 0, "cacheScope": "public"}
            elif method == "tools/list":
                result = {"resultType": "complete", "ttlMs": 0, "cacheScope": "public", "tools": [{
                    "name": self.TOOL, "title": "Wait on the pull request",
                    "description": "Waits in the background until the pull request is merged or closed.",
                    "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
                    "annotations": {"readOnlyHint": True, "openWorldHint": True}}]}
            elif method == "tools/call":
                task_id = self.last_task = f"wait-{len(self.tasks) + 1}"
                self.tasks[task_id] = "working"
                result = {**self.task(task_id), "resultType": "task"}
            elif method in ("tasks/get", "tasks/cancel") and params.get("taskId") in self.tasks:
                if method == "tasks/cancel" and self.tasks[params["taskId"]] == "working":
                    self.tasks[params["taskId"]] = "cancelled"
                result = self.task(params["taskId"])
            elif method == "ping":
                result = {"resultType": "complete"}
            else:
                return {"jsonrpc": "2.0", "id": body["id"], "error": {"code": -32601, "message": f"no {method}"}}
        return {"jsonrpc": "2.0", "id": body["id"], "result": result}

    def end(self, task_id):
        with self.lock:
            self.tasks[task_id] = "completed"

    def call_off(self, task_id):
        with self.lock:
            self.tasks[task_id] = "cancelled"


def is_listing_a_task(value):
    return value.get("type") == "system" and value.get("subtype") == "background_tasks_changed" and value.get("tasks")


def call_the_tool(cli, server):
    """Asks for one call to the tool, allows it if asked, and waits for the turn to end with its task listed."""
    cli.send(user(f"Call the mcp__roundhouse__{TaskServer.TOOL} tool once, with no arguments. Then reply with the single "
                  "word WAITING, and nothing else."))
    if is_approval(asked := cli.wait_for(lambda v: is_approval(v) or is_listing_a_task(v))):
        respond(cli, asked, {"behavior": "allow", "updatedInput": asked["request"]["input"]})
        cli.wait_for(is_listing_a_task)
    cli.wait_for(is_result)
    return server.last_task


def capture_mcp_task(cwd):
    """A task that ends while the CLI runs, which it picks up after by itself. Then another, the CLI stopped with
    SIGTERM while it's listed, as a worker stops its agents as its hub goes, and resumed, as the hub starts it again
    once the wait ends, which lists it again and asks after it. Then a third, which the server calls off, as the hub
    does for the person."""
    server = TaskServer()
    config = pathlib.Path(cwd) / ".mcp-config.json"
    config.write_text(json.dumps({"mcpServers": {"roundhouse": {"type": "http", "url": server.url,
                                                                "headers": {"Authorization": "Bearer stand-in"}}}}))
    takes_tasks = {"CLAUDE_AUTO_BACKGROUND_TASKS": "1"}
    # Only the stand-in, so none of this machine's own servers are reached or written down.
    servers = ["--mcp-config", str(config), "--strict-mcp-config"]
    cli = Cli(cwd, servers, takes_tasks)
    first = call_the_tool(cli, server)
    server.end(first)
    cli.wait_for(is_result)
    time.sleep(0.5)
    second = call_the_tool(cli, server)
    session_id = next(v["session_id"] for v in cli.lines if v.get("type") == "system" and v.get("subtype") == "init")
    time.sleep(1)
    before = len(cli.lines)
    cli.stop(signal.SIGTERM)
    stopped = list(cli.lines)
    print("written on SIGTERM:", [(v.get("type"), v.get("subtype"), v.get("tasks")) for v in stopped[before:]])
    print("asked of the stand-in before the CLI stopped:", server.asked)
    if server.tasks[second] != "working":
        print(f"the CLI left the task {server.tasks[second]} as it stopped")
    server.asked = []

    resumed = Cli(cwd, [*servers, "--resume", session_id], takes_tasks)
    resumed.wait_for(is_listing_a_task, timeout=60)
    time.sleep(1.5)
    server.end(second)
    resumed.wait_for(is_result)
    time.sleep(0.5)
    third = call_the_tool(resumed, server)
    before = len(resumed.lines)
    server.call_off(third)
    # The turn the CLI takes by itself once it hears, the first since the one that called the tool.
    resumed.wait_for(is_result)
    time.sleep(0.3)
    print("written once the server called its task off:",
          [(v.get("type"), v.get("subtype"), v.get("status"), v.get("tasks")) for v in resumed.lines[before:]])
    resumed.stop(signal.SIGKILL)
    print("asked of the stand-in once resumed:", server.asked)
    return stopped, resumed.lines


def keep_only_whats_applied(value):
    """`get_settings` answers with the user's own settings too, which are personal and which the adapter doesn't read."""
    answer = value.get("response", {}).get("response")
    if value.get("type") == "control_response" and isinstance(answer, dict) and "applied" in answer:
        answer["effective"], answer["sources"] = {}, []
    return value


def keep_only_what_the_context_holds(value):
    """`get_context_usage` also lists the user's skills, memory files and MCP tools by name and path, and the grid
    `/context` draws. The adapter reads only how much is used, of how much, and each category."""
    answer = value.get("response", {}).get("response")
    if value.get("type") == "control_response" and isinstance(answer, dict) and "categories" in answer:
        kept = {key: answer[key] for key in ("totalTokens", "maxTokens") if key in answer}
        kept["categories"] = [{key: category[key] for key in ("id", "name", "tokens", "kind") if key in category}
                              for category in answer["categories"]]
        value["response"]["response"] = kept
    return value


def made_up_usage(value):
    """`get_usage` answers with how much of the account's limits is used, how it was used, by day and week, and what
    it was spent on, which is all personal. This keeps whether it says there are limits to tell of, and which windows
    the account has, each with only the keys the adapter reads, with made-up numbers, as `made_up_limits` does. It
    names many more, with `null` for each the account doesn't have, which are left out."""
    answer = value.get("response", {}).get("response")
    if value.get("type") == "control_response" and isinstance(answer, dict) and "rate_limits" in answer:
        made_up = {"utilization": 25, "resets_at": "2100-01-01T00:00:00+00:00"}
        windows = answer["rate_limits"] if isinstance(answer["rate_limits"], dict) else {}
        kept = {name: {key: made_up[key] for key in made_up if key in window}
                for name, window in windows.items() if isinstance(window, dict)}
        value["response"]["response"] = {"rate_limits_available": answer.get("rate_limits_available"),
                                         "rate_limits": kept}
    return value


def made_up_limits(info):
    """A `rate_limit_event` says how much of the account's limits is used, which is personal. This keeps which
    windows it named, and only the keys the adapter reads, with made-up numbers, so the adapter's reading of it is
    still checked and nothing new the CLI adds about the account gets in."""
    made_up = {"utilization": 0.25, "resetsAt": 4102444800}

    def kept(window):
        return {key: made_up[key] for key in made_up if key in window}

    limits = {"status": "allowed", **kept(info)}
    if isinstance(info.get("rateLimitType"), str):
        limits["rateLimitType"] = info["rateLimitType"]
    if isinstance(info.get("unifiedWindows"), dict):
        limits["unifiedWindows"] = {name: kept(window) for name, window in info["unifiedWindows"].items()
                                    if isinstance(window, dict)}
    return limits


def scrub(text, cwd):
    # A task's output file is under a directory named for the conversation's, with a dash for anything but a letter
    # or a digit.
    for path in (os.path.realpath(cwd), str(cwd)):
        text = text.replace(re.sub(r"[^A-Za-z0-9]", "-", path), "-tmp-fixture")
    # Which user this is, by number, which names the CLI's directory of task output.
    if hasattr(os, "getuid"):
        text = text.replace(f"/claude-{os.getuid()}/", "/claude-uid/")
    text = text.replace(os.path.realpath(cwd), "/tmp/fixture").replace(str(cwd), "/tmp/fixture")
    return text.replace(str(pathlib.Path.home()), "/home/user")


def scrub_streamed_text(lines, cwd):
    """Streaming can split a path anywhere, so each block is scrubbed whole and cut up again the same way."""
    blocks = {}
    for value in lines:
        if value.get("type") == "stream_event" and value["event"].get("type") == "content_block_delta":
            delta = value["event"]["delta"]
            for field in ("partial_json", "text", "thinking"):
                if field in delta:
                    key = (value.get("api_message_id"), value["event"].get("index"), field)
                    blocks.setdefault(key, []).append(delta)
    for (_, _, field), deltas in blocks.items():
        text = scrub("".join(delta[field] for delta in deltas), cwd)
        for i, delta in enumerate(deltas):
            delta[field] = text[i * len(text) // len(deltas):(i + 1) * len(text) // len(deltas)]


def sanitize(value, cwd):
    """Keeps the shape and drops anything about this machine or account."""
    if value.get("type") == "system" and value.get("subtype") == "init":
        for key in ("tools", "mcp_servers", "slash_commands", "terminal_slash_commands", "skills", "plugins", "agents", "additional_directories"):
            if key in value:
                value[key] = []
        for key in ("memory_paths", "messaging_socket_path"):
            value.pop(key, None)
    if value.get("type") == "rate_limit_event":
        value["rate_limit_info"] = made_up_limits(value.get("rate_limit_info", {}))
    text = json.dumps(value)
    text = scrub(text, cwd)
    # Opaque and long. They're never read, so a stub keeps the file small.
    text = re.sub(r'"signature": "[^"]{16,}"', '"signature": "stub"', text)
    return json.loads(text)


def refuse_anything_personal(lines):
    """The last check before this goes in the repo, including across chunks of streamed text."""
    name = pathlib.Path.home().name.lower()
    streamed = "".join(v["event"]["delta"].get("partial_json", "") + v["event"]["delta"].get("text", "")
                       for v in lines if v.get("type") == "stream_event" and "delta" in v["event"])
    if name in streamed.lower() or any(name in json.dumps(v).lower() for v in lines):
        sys.exit(f"the capture still mentions {name!r}, so nothing was written")


def kind_of(value):
    if value.get("type") != "system":
        return None
    # Mode changes are few, and what the status lines are kept for.
    return "mode" if value.get("subtype") == "status" and "permissionMode" in value else value.get("subtype")


def write(path, lines):
    refuse_anything_personal(lines)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(json.dumps(value) + "\n" for value in lines))


def main():
    only = sys.argv[1] if sys.argv[1:] in (["--controls"], ["--background"], ["--mcp-task"]) else None
    with tempfile.TemporaryDirectory(dir="/tmp") as cwd:
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=cwd, check=True)
        if only == "--mcp-task":
            captured = [(path, kept_of(lines, cwd)) for path, lines in zip((MCP_TASK, MCP_TASK_RESUMED),
                                                                           capture_mcp_task(cwd))]
            forget_the_conversations(cwd)
            for path, kept in captured:
                write(path, kept)
                print(f"wrote {len(kept)} lines from claude {version()} to {path}")
            return
        kept = kept_of(capture(cwd), cwd) if only is None else []
        background = kept_of(capture_background(cwd), cwd) if only in (None, "--background") else []
        controls = []
        if only in (None, "--controls"):
            controls = [sanitize(made_up_usage(keep_only_what_the_context_holds(keep_only_whats_applied(value))), cwd)
                        for value in capture_controls(cwd)]
        forget_the_conversations(cwd)
    for path, lines in ((FIXTURE, kept), (BACKGROUND, background), (CONTROLS, controls)):
        if lines:
            write(path, lines)
            print(f"wrote {len(lines)} lines from claude {version()} to {path}")


def kept_of(lines, cwd):
    """What goes in a fixture of the lines captured: streamed text scrubbed whole, at most `MAX_PER_KIND` of the
    kinds it names, and each line sanitized."""
    scrub_streamed_text(lines, cwd)
    counts, kept = {}, []
    for value in lines:
        kind = kind_of(value)
        counts[kind] = counts.get(kind, 0) + 1
        if kind in MAX_PER_KIND and counts[kind] > MAX_PER_KIND[kind]:
            continue
        kept.append(sanitize(value, cwd))
    return kept


def forget_the_conversations(cwd):
    """The CLI keeps a conversation file per directory, named for its path with anything but a letter or a digit made
    a dash, as an underscore in a random name can be. The random name makes this exact."""
    projects = pathlib.Path.home() / ".claude/projects"
    for leftover in projects.glob(f"*{re.sub(r'[^A-Za-z0-9]', '-', pathlib.Path(cwd).name)}*"):
        shutil.rmtree(leftover, ignore_errors=True)


def version():
    return subprocess.run(["claude", "--version"], capture_output=True, text=True).stdout.split()[0]


if __name__ == "__main__":
    main()
