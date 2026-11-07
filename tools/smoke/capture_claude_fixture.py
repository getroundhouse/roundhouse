#!/usr/bin/env python3
"""Regenerates crates/agents/tests/fixtures/claude.jsonl from the real `claude` CLI.

Run this after upgrading the CLI, then run `cargo test -p agents`. A failure there means the CLI's
output changed in a way the adapter depends on.

It uses the adapter's exact flags in a scratch repo, with trivial prompts, on a cheap model. Costs a
fraction of a cent. Machine details and personal configuration are removed before anything is
written, because the output goes in the repo.
"""
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "crates/agents/tests/fixtures/claude.jsonl"
FLAGS = """-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages
--thinking-display summarized --permission-prompt-tool stdio --allow-dangerously-skip-permissions --permission-mode manual --model haiku --effort high""".split()
# Keeps the fixture small. Order is preserved, so the protocol's sequencing is still real.
MAX_PER_KIND = {"thinking_tokens": 2, "status": 3, "hook_started": 0, "hook_response": 0}


def user(text):
    return {"type": "user", "session_id": "", "parent_tool_use_id": None,
            "message": {"role": "user", "content": [{"type": "text", "text": text}]}}


class Cli:
    def __init__(self, cwd):
        scrub = ("CLAUDECODE", "CLAUDE_AGENT_SDK_VERSION", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_ENTRYPOINT",
                 "CLAUDE_CODE_EXECPATH", "CLAUDE_CODE_MESSAGING_SOCKET", "CLAUDE_CODE_MESSAGING_TOKEN",
                 "CLAUDE_CODE_SESSION_ATTENDED", "CLAUDE_CODE_SESSION_ID", "CLAUDE_EFFORT", "CLAUDE_PID")
        env = {k: v for k, v in os.environ.items() if k not in scrub}
        env["CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS"] = "1"
        self.proc = subprocess.Popen(["claude", *FLAGS], cwd=cwd, env=env, text=True,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.lines, self.seen = [], []
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            value = json.loads(line)
            self.lines.append(value)
            self.seen.append(value)

    def send(self, value):
        self.proc.stdin.write(json.dumps(value) + "\n")
        self.proc.stdin.flush()

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


def scrub(text, cwd):
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
        value["rate_limit_info"] = {"status": "allowed", "rateLimitType": "seven_day"}
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


def main():
    with tempfile.TemporaryDirectory(dir="/tmp") as cwd:
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=cwd, check=True)
        lines = capture(cwd)
        scrub_streamed_text(lines, cwd)
        kept, counts = [], {}
        for value in lines:
            kind = kind_of(value)
            counts[kind] = counts.get(kind, 0) + 1
            if kind in MAX_PER_KIND and counts[kind] > MAX_PER_KIND[kind]:
                continue
            kept.append(sanitize(value, cwd))
        # The CLI keeps a conversation file per directory. The random name makes this exact.
        projects = pathlib.Path.home() / ".claude/projects"
        for leftover in projects.glob(f"*{pathlib.Path(cwd).name}*"):
            shutil.rmtree(leftover, ignore_errors=True)
    refuse_anything_personal(kept)
    version = next(v["claude_code_version"] for v in kept if kind_of(v) == "init")
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    FIXTURE.write_text("".join(json.dumps(value) + "\n" for value in kept))
    print(f"wrote {len(kept)} lines from claude {version} to {FIXTURE}")


if __name__ == "__main__":
    main()
