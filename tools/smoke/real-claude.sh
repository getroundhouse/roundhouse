#!/usr/bin/env bash
# The whole stack against the real `claude` CLI: step 1.1's "done when", plus the behaviors the fake
# agent can only imitate. Makes about 15 small Haiku calls on your own login, so run it by hand
# rather than in CI. Uses a throwaway home, repository and worktrees under /tmp.
#
#   cargo build --workspace && tools/smoke/real-claude.sh
set -u

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${BIN:-$ROOT/target/debug}"
WORK="$(mktemp -d /tmp/ahsmoke-XXXXXX)"
export AGENTHUB_HOME="$WORK/home"
export AGENTHUB_WORKSPACES="$WORK/workspaces"
REPO="$WORK/myrepo"
MODEL="${MODEL:-haiku}"
failures=0
passes=0

cli() { "$BIN/agenthub" "$@"; }
pass() { passes=$((passes + 1)); echo "  ok    $1"; }
fail() { failures=$((failures + 1)); echo "  FAIL  $1"; }
check() { if eval "$2"; then pass "$1"; else fail "$1"; fi; }
section() { echo; echo "== $1"; }

# Evaluates a Python expression over a session's events, each a dict with `seq` and `body`.
q() {
  cli log "$1" --raw --limit 2000 | python3 -I -c "
import json, sys
events = [json.loads(line) for line in sys.stdin]
kinds = [e['body']['type'] for e in events]
def pid():
    started = [e['body']['pid'] for e in events if e['body']['type'] == 'agent_started']
    return started[-1] if started else ''
def text():
    return ''.join(b.get('text', '') for e in events if e['body']['type'] == 'assistant' for b in e['body']['blocks'] if b['type'] == 'text')
print($2)"
}

wait_for() { # description, session, python expression that's truthy when done, timeout
  local deadline=$((SECONDS + ${4:-90}))
  while [ $SECONDS -lt $deadline ]; do
    if [ "$(q "$2" "bool($3)" 2>/dev/null)" = "True" ]; then pass "$1"; return 0; fi
    sleep 0.3
  done
  fail "$1 (timed out)"; return 1
}

alive() { kill -0 "$1" 2>/dev/null; }
session_status() { cli session list "$1" | tail -1 | awk '{print $2}'; }
wait_idle() { # workspace
  local deadline=$((SECONDS + 90))
  while [ $SECONDS -lt $deadline ]; do [ "$(session_status "$1")" = "Idle" ] && return 0; sleep 0.3; done
  return 1
}
wait_gone() { # pid
  local deadline=$((SECONDS + 15))
  while [ $SECONDS -lt $deadline ]; do alive "$1" || return 0; sleep 0.1; done
  return 1
}
slow_tool() { echo "Use the Bash tool to run exactly: python3 -c \"import time; time.sleep($1)\"; echo $2 > $3   Then reply with the single word DONE."; }

# The CLI keeps its own conversation files per worktree. The random suffix makes this exact.
cleanup() {
  cli hub stop >/dev/null 2>&1
  sleep 1
  rm -rf "$WORK"
  rm -rf "$HOME/.claude/projects/"*"${WORK##*/}"*
}
trap cleanup EXIT

mkdir -p "$REPO"
git -C "$REPO" init -q -b main
echo hello > "$REPO/a.txt"
git -C "$REPO" add .
git -C "$REPO" -c user.name=t -c user.email=t@example.com commit -qm first

section "A real session in a real worktree"
started_ms() { python3 -I -c 'import time; print(int(time.time()*1000))'; }
cli repo add "$REPO" >/dev/null
t0=$(started_ms)
cli workspace create myrepo --name smoke >/dev/null
echo "  time  creating a workspace, including starting the hub: $(($(started_ms) - t0)) ms"
WT="$AGENTHUB_WORKSPACES/myrepo/smoke"
check "the worktree exists on its own branch" '[ "$(git -C "$WT" branch --show-current)" = smoke ]'
t0=$(started_ms)
cli workspace create myrepo --name timing >/dev/null
echo "  time  creating a second workspace: $(($(started_ms) - t0)) ms"
SID=$(cli session new smoke --model "$MODEL" --permission-mode bypassPermissions)

section "Surviving a client restart"
"$BIN/agenthub" send "$SID" "$(slow_tool 12 survived survived.txt)" > "$WORK/client1.out" 2>&1 &
CLIENT=$!
wait_for "the agent starts a tool while a client is attached" "$SID" "any(b['type'] == 'tool_use' for e in events if e['body']['type'] == 'assistant' for b in e['body']['blocks'])"
kill -9 "$CLIENT" 2>/dev/null; wait "$CLIENT" 2>/dev/null
SEEN=$(q "$SID" "len(events)")
AGENT=$(q "$SID" "pid()")
WORKER=$(ps -o ppid= -p "$AGENT" | tr -d ' ')
HUB=$(ps -o ppid= -p "$WORKER" | tr -d ' ')
HUB_REPORTED=$(cli hub status | awk '{print $2}')
check "the agent is a real claude process in the worktree's session" 'ps -o command= -p "$AGENT" | grep -q claude'
check "the worker is a separate process from the agent and the hub" '[ "$WORKER" != "$AGENT" ] && [ "$WORKER" != "$HUB" ] && ps -o command= -p "$WORKER" | grep -q "agenthubd worker"'
check "the hub is the worker's parent" '[ "$HUB" = "$HUB_REPORTED" ] && ps -o command= -p "$HUB" | grep -q "agenthubd hub"'
check "the agent is still running with the client gone" 'alive "$AGENT"'
check "the session still reports running" '[ "$(session_status smoke)" = Running ]'
check "the hub sees only the client asking" 'cli hub status | grep -q "1 client(s)"'
cli attach "$SID" --from $((SEEN + 1)) --until-idle > "$WORK/client2.out" 2>&1
check "a new client catches up from where the old one stopped" 'grep -q "turn finished" "$WORK/client2.out"'
check "the catch-up repeats nothing the old client saw" '! grep -q "^\[$SEEN\]" "$WORK/client2.out"'
check "the log has no gaps" '[ "$(q "$SID" "[e[\"seq\"] for e in events] == list(range(1, len(events) + 1))")" = True ]'
check "the file landed in the worktree" '[ -f "$WT/survived.txt" ]'
check "and not in the repository" '[ ! -f "$REPO/survived.txt" ]'
check "raw agent lines were kept" '[ "$(q "$SID" "any(\"raw\" in e and json.loads(e[\"raw\"]).get(\"type\") == \"assistant\" for e in events)")" = True ]'

section "The conversation continues"
OUT=$(cli send "$SID" "What file did you just create? Reply with only its file name." 2>&1)
check "the same agent answers from context" 'echo "$OUT" | grep -q survived.txt'
check "without starting a new process" '[ "$(q "$SID" "kinds.count(\"agent_started\")")" = 1 ]'
check "status comes from the worker" 'cli workspace status smoke | grep -q "?? *survived.txt"'
check "diff comes from the worker" 'cli workspace diff smoke | grep -q "+survived"'

section "Resource use, one idle agent"
for pair in "hub:$HUB" "worker:$WORKER" "agent:$AGENT"; do
  echo "  rss   ${pair%%:*}: $(( $(ps -o rss= -p "${pair##*:}") / 1024 )) MB"
done

section "Approvals"
ASID=$(cli session new timing --model "$MODEL" --permission-mode manual)
WT2="$AGENTHUB_WORKSPACES/myrepo/timing"
cli send "$ASID" "Use the Bash tool to run exactly: echo approved > approved.txt   Then reply with the single word DONE." --no-follow >/dev/null
wait_for "the agent asks for approval" "$ASID" "'approval_requested' in kinds"
check "the tool hasn't run yet" '[ ! -f "$WT2/approved.txt" ]'
check "the session reports waiting" '[ "$(session_status timing)" = WaitingApproval ]'
APPROVAL=$(q "$ASID" "[e['body']['approval_id'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
cli approve "$ASID" "$APPROVAL"
check "it runs once approved" 'wait_idle timing && [ -f "$WT2/approved.txt" ]'
cli send "$ASID" "Use the Bash tool to run exactly: echo denied > denied.txt" --no-follow >/dev/null
wait_for "it asks again" "$ASID" "kinds.count('approval_requested') == 2"
APPROVAL=$(q "$ASID" "[e['body']['approval_id'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
cli approve "$ASID" "$APPROVAL" --deny "Not this time"
check "a denied tool doesn't run" 'wait_idle timing && [ ! -f "$WT2/denied.txt" ]'
check "the agent is told why" '[ "$(q "$ASID" "any(b.get(\"is_error\") and \"Not this time\" in b.get(\"text\", \"\") for e in events if e[\"body\"][\"type\"] == \"agent_user\" for b in e[\"body\"][\"blocks\"])")" = True ]'

section "Cancelling"
cli send "$SID" "$(slow_tool 41 cancelled cancelled.txt)" --no-follow >/dev/null
wait_for "a long tool is running" "$SID" "'41' in str(events[-5:]) and any(b['type'] == 'tool_use' for e in events[-4:] if e['body']['type'] == 'assistant' for b in e['body']['blocks'])"
sleep 1
cli cancel "$SID"
wait_for "the turn ends as cancelled" "$SID" "[e['body'] for e in events if e['body']['type'] == 'turn_ended'][-1]['outcome'] == 'cancelled'" 30
check "the tool stopped" '! pgrep -f "time.sleep\(41\)" >/dev/null'
check "the file was never written" '[ ! -f "$WT/cancelled.txt" ]'
check "the same agent process is still up" 'alive "$AGENT"'
OUT=$(cli send "$SID" "Reply with the single word ALIVE." 2>&1)
check "and takes the next prompt" 'echo "$OUT" | grep -q ALIVE'

section "The hub crashing mid-turn"
cli send "$SID" "$(slow_tool 43 crashed crashed.txt)" --no-follow >/dev/null
wait_for "a long tool is running" "$SID" "any(b['type'] == 'tool_use' and '43' in str(b['input']) for e in events if e['body']['type'] == 'assistant' for b in e['body']['blocks'])"
sleep 1
HUB=$(cli hub status | awk '{print $2}')
kill -9 "$HUB"
check "the worker exits" 'wait_gone "$WORKER"'
check "the agent exits" 'wait_gone "$AGENT"'
check "its tool exits" 'sleep 1; ! pgrep -f "time.sleep\(43\)" >/dev/null'
check "with no hub running, status says so rather than starting one" 'cli hub status | grep -q "no hub"'
check "the next command starts a new hub, and the session is interrupted" '[ "$(session_status smoke)" = Interrupted ]'
check "recovery recorded why" '[ "$(q "$SID" "any(e[\"body\"][\"type\"] == \"agent_exited\" and e[\"body\"][\"reason\"] == \"hub_restart\" for e in events)")" = True ]'
OUT=$(cli send "$SID" "What was the first file you created in this conversation? Reply with only its file name." 2>&1)
check "the resumed agent still has the conversation" 'echo "$OUT" | grep -q survived.txt'
check "it was resumed, not restarted" '[ "$(q "$SID" "[e[\"body\"][\"resumed\"] for e in events if e[\"body\"][\"type\"] == \"agent_started\"][-1]")" = True ]'
AGENT=$(q "$SID" "pid()")

section "A clean stop and a hub started the way Finder would"
cli hub stop >/dev/null
check "the agent stops with the hub" 'wait_gone "$AGENT"'
sleep 0.5
env -i HOME="$HOME" USER="$USER" PATH=/usr/bin:/bin AGENTHUB_HOME="$AGENTHUB_HOME" AGENTHUB_WORKSPACES="$AGENTHUB_WORKSPACES" \
  "$BIN/agenthubd" hub >>"$WORK/finder-hub.log" 2>&1 &
deadline=$((SECONDS + 10))
until cli hub status 2>/dev/null | grep -q pid || [ $SECONDS -ge $deadline ]; do sleep 0.1; done
OUT=$(cli send "$SID" "Reply with the single word FINDER." 2>&1)
check "with a minimal environment it still finds the CLI and keeps the conversation" 'echo "$OUT" | grep -q FINDER'
AGENT=$(q "$SID" "pid()")
WORKER=$(ps -o ppid= -p "$AGENT" | tr -d ' ')
check "and the worker got the login shell's PATH, not the minimal one" 'ps eww -o command= -p "$WORKER" | tr " " "\n" | grep "^PATH=" | grep -qv "^PATH=/usr/bin:/bin$"'

section "Archive and restore"
cli workspace archive smoke >"$WORK/archive.out" 2>&1
check "uncommitted work blocks archiving" 'grep -q "uncommitted" "$WORK/archive.out" && [ -f "$WT/survived.txt" ]'
git -C "$WT" add . && git -C "$WT" -c user.name=t -c user.email=t@example.com commit -qm "the agent's work"
AGENT=$(q "$SID" "pid()")
cli workspace archive smoke >/dev/null
check "archiving removes the worktree" '[ ! -d "$WT" ]'
check "and stops the agent" 'wait_gone "$AGENT"'
check "and keeps the branch" 'git -C "$REPO" rev-parse --verify --quiet refs/heads/smoke >/dev/null'
check "the history is still readable" '[ "$(q "$SID" "len(events) > 20")" = True ]'
cli workspace restore smoke >/dev/null
check "restoring brings the work back" '[ -f "$WT/survived.txt" ]'
OUT=$(cli send "$SID" "What was the first file you created in this conversation? Reply with only its file name." 2>&1)
check "and the conversation with it" 'echo "$OUT" | grep -q survived.txt'

section "Cost of this run"
q "$SID" "'  session 1: \${:.4f}'.format(sum(e['body'].get('cost_usd') or 0 for e in events if e['body']['type'] == 'turn_ended'))"
q "$ASID" "'  session 2: \${:.4f}'.format(sum(e['body'].get('cost_usd') or 0 for e in events if e['body']['type'] == 'turn_ended'))"

echo
echo "$passes passed, $failures failed"
[ "$failures" -eq 0 ]
