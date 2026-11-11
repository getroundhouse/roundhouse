#!/usr/bin/env bash
# The whole stack against the real `claude` CLI: step 1.1's "done when", plus the behaviors the fake
# agent can only imitate. Makes about 25 small Haiku calls, a few more that title chats, and on Sonnet,
# three plans and a turn in plan mode, since plan mode runs on Sonnet, and one reply, on your own
# login, so run it by hand rather than in CI. Uses a throwaway home, repository and worktrees under /tmp.
#
#   cargo build --workspace && tools/smoke/real-claude.sh
set -u

# For target_dir, since cargo builds wherever `build.target-dir` or CARGO_TARGET_DIR says.
# shellcheck source=SCRIPTDIR/../build.sh
source "$(dirname "$0")/../build.sh"
BIN="${BIN:-$(target_dir)/debug}"
# Cleanup deletes by this name, so an empty one would match every conversation the CLI has.
WORK="$(mktemp -d /tmp/ahsmoke-XXXXXX)" || exit 1
export ROUNDHOUSE_HOME="$WORK/home"
export ROUNDHOUSE_WORKSPACES="$WORK/workspaces"
REPO="$WORK/myrepo"
MODEL="${MODEL:-haiku}"
failures=0
passes=0

cli() { "$BIN/roundhouse" "$@"; }
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

# Where a workspace's title and its first chat's came from, as the hub stored them, since the CLI doesn't print that.
title_sources() { # workspace name
  python3 -I -c "
import sqlite3, sys
db = sqlite3.connect('file:' + sys.argv[1] + '?mode=ro', uri=True)
workspace, source = db.execute('SELECT id, title_source FROM workspaces WHERE name = ?', (sys.argv[2],)).fetchone()
chat = db.execute('SELECT title_source FROM sessions WHERE workspace_id = ? ORDER BY created_at_ms', (workspace,)).fetchone()
print(source, chat[0])" "$ROUNDHOUSE_HOME/hub.db" "$1"
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
# For when a workspace holds more than one chat, and "the last one listed" won't do.
session_status_of() { cli session list timing | grep "^${1:0:8}" | awk '{print $2}'; }
wait_until_idle_session() { # session
  local deadline=$((SECONDS + 90))
  while [ $SECONDS -lt $deadline ]; do [ "$(session_status_of "$1")" = "Idle" ] && return 0; sleep 0.3; done
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
WT="$ROUNDHOUSE_WORKSPACES/myrepo/smoke"
check "the worktree exists on its own branch" '[ "$(git -C "$WT" branch --show-current)" = smoke ]'
t0=$(started_ms)
cli workspace create myrepo --name timing >/dev/null
echo "  time  creating a second workspace: $(($(started_ms) - t0)) ms"
SID=$(cli session new smoke --model "$MODEL" --permission-mode bypassPermissions)

section "Surviving a client restart"
"$BIN/roundhouse" send "$SID" "$(slow_tool 12 survived survived.txt)" > "$WORK/client1.out" 2>&1 &
CLIENT=$!
wait_for "the agent starts a tool while a client is attached" "$SID" "any(b['type'] == 'tool_use' for e in events if e['body']['type'] == 'assistant' for b in e['body']['blocks'])"
kill -9 "$CLIENT" 2>/dev/null; wait "$CLIENT" 2>/dev/null
SEEN=$(q "$SID" "len(events)")
AGENT=$(q "$SID" "pid()")
WORKER=$(ps -o ppid= -p "$AGENT" | tr -d ' ')
HUB=$(ps -o ppid= -p "$WORKER" | tr -d ' ')
HUB_REPORTED=$(cli hub status | awk '{print $2}')
check "the agent is a real claude process in the worktree's session" 'ps -o command= -p "$AGENT" | grep -q claude'
check "the worker is a separate process from the agent and the hub" '[ "$WORKER" != "$AGENT" ] && [ "$WORKER" != "$HUB" ] && ps -o command= -p "$WORKER" | grep -q "roundhoused worker"'
check "the hub is the worker's parent" '[ "$HUB" = "$HUB_REPORTED" ] && ps -o command= -p "$HUB" | grep -q "roundhoused hub"'
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

check "the first prompt's title was generated, for the workspace and its chat" '[ "$(title_sources smoke)" = "generated generated" ]'

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

section "Approvals, in a chat that starts in plan mode"
ASID=$(cli session new timing --model "$MODEL" --permission-mode plan)
WT2="$ROUNDHOUSE_WORKSPACES/myrepo/timing"
plan_for() { echo "Make a one line plan to create a file $1 containing hello, then call ExitPlanMode. Do not ask questions."; }
cli send "$ASID" "$(plan_for approved.txt)" --no-follow >/dev/null
wait_for "the plan comes back for approval" "$ASID" "any(e['body']['type'] == 'approval_requested' and e['body']['tool_name'] == 'ExitPlanMode' for e in events)" 120
check "nothing has been done yet" '[ ! -f "$WT2/approved.txt" ]'
check "the session reports waiting" '[ "$(session_status timing)" = WaitingApproval ]'
check "its first turn was in plan mode" '[ "$(q "$ASID" "[e[\"body\"][\"permission_mode\"] for e in events if e[\"body\"][\"type\"] == \"agent_init\"][0]")" = plan ]'
check "and going into it at launch isn't reported as a change" '[ "$(q "$ASID" "kinds.count(\"agent_mode\")")" = 0 ]'
APPROVAL=$(q "$ASID" "[e['body']['approval_id'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
cli approve "$ASID" "$APPROVAL" --deny "Not this time"
check "a plan sent back isn't carried out" 'wait_idle timing && [ ! -f "$WT2/approved.txt" ]'
check "the agent is told why" '[ "$(q "$ASID" "any(b.get(\"is_error\") and \"Not this time\" in b.get(\"text\", \"\") for e in events if e[\"body\"][\"type\"] == \"agent_user\" for b in e[\"body\"][\"blocks\"])")" = True ]'
check "and it's still planning" '[ "$(q "$ASID" "kinds.count(\"agent_mode\")")" = 0 ]'
cli send "$ASID" "$(plan_for approved.txt)" --no-follow >/dev/null
wait_for "it asks again" "$ASID" "kinds.count('approval_requested') == 2" 120
APPROVAL=$(q "$ASID" "[e['body']['approval_id'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
cli approve "$ASID" "$APPROVAL"
wait_for "approving it goes to full access, not to asking first" "$ASID" "[e['body']['permission_mode'] for e in events if e['body']['type'] == 'agent_mode'] == ['bypassPermissions']"
check "and the plan is carried out without asking" 'wait_idle timing && [ -f "$WT2/approved.txt" ] && [ "$(q "$ASID" "kinds.count(\"approval_requested\")")" = 2 ]'

section "Changing the model, plan mode and effort"
MSID=$(cli session new timing --model "$MODEL")
mode_of() { cli session list timing | grep "^${MSID:0:8}" | sed 's/.*mode=\([^ ]*\).*/\1/'; }
OUT=$(cli send "$MSID" "Reply with the single word READY." 2>&1)
check "a new chat runs beside the other one" 'echo "$OUT" | grep -q READY'
check "the first prompt names it" 'cli session list timing | grep "^${MSID:0:8}" | grep -q "Reply with the single word READY."'
MAGENT=$(q "$MSID" "pid()")
OUT=$(cli send "$MSID" "Use the Bash tool to run exactly: echo open > full-access.txt   Then reply with the single word DONE." 2>&1)
check "a chat with no mode chosen has full access" '[ -f "$WT2/full-access.txt" ]'
check "it didn't ask" '[ "$(q "$MSID" "kinds.count(\"approval_requested\")")" = 0 ]'
check "no other mode is taken" '! cli session set "$MSID" --permission-mode manual >/dev/null 2>&1 && [ "$(mode_of)" = bypassPermissions ]'
OUT=$(cli session set "$MSID" --model not-a-real-model 2>&1)
check "a model the CLI doesn't know is refused, with its reason" 'echo "$OUT" | grep -q "not found"'
check "and nothing changed" 'cli session list timing | grep "^${MSID:0:8}" | grep -q "model=$MODEL"'
check "the agent survives a refusal" 'alive "$MAGENT"'
cli session set "$MSID" --permission-mode plan >/dev/null
check "plan mode is reported back" '[ "$(q "$MSID" "[e[\"body\"][\"permission_mode\"] for e in events if e[\"body\"][\"type\"] == \"agent_mode\"][-1]")" = plan ]'
check "by the same process" '[ "$(q "$MSID" "pid()")" = "$MAGENT" ]'
# The agent may make a plan rather than call the tool, so either is fine, as long as nothing runs without asking.
ASKED=$(q "$MSID" "kinds.count('approval_requested')")
TURNS=$(q "$MSID" "kinds.count('turn_ended')")
cli send "$MSID" "Use the Bash tool to run exactly: echo asked > asked.txt   Then reply with the single word DONE." --no-follow >/dev/null
wait_for "in plan mode, it asks before using a tool, or doesn't use one" "$MSID" "kinds.count('approval_requested') > $ASKED or kinds.count('turn_ended') > $TURNS" 120
if [ "$(q "$MSID" "kinds.count('approval_requested')")" -gt "$ASKED" ]; then
  APPROVAL=$(q "$MSID" "[e['body']['approval_id'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
  cli approve "$MSID" "$APPROVAL" --deny "Not while planning"
fi
check "and nothing ran" 'wait_until_idle_session "$MSID" && [ ! -f "$WT2/asked.txt" ]'
cli send "$MSID" "$(plan_for planned.txt)" --no-follow >/dev/null
wait_for "the plan comes back for approval" "$MSID" "any(e['body']['type'] == 'approval_requested' and e['body']['tool_name'] == 'ExitPlanMode' for e in events)" 120
check "nothing has been done yet" '[ ! -f "$WT2/planned.txt" ]'
APPROVAL=$(q "$MSID" "[e['body']['approval_id'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
cli approve "$MSID" "$APPROVAL"
wait_for "approving it leaves plan mode, by the CLI's own report" "$MSID" "[e['body']['permission_mode'] for e in events if e['body']['type'] == 'agent_mode'][-1] != 'plan'"
AFTER_PLAN=$(mode_of)
check "a plan chosen from full access goes back to it" '[ "$AFTER_PLAN" = bypassPermissions ]'
check "the hub's record follows it" '[ "$AFTER_PLAN" = "$(q "$MSID" "[e[\"body\"][\"permission_mode\"] for e in events if e[\"body\"][\"type\"] == \"agent_mode\"][-1]")" ]'
wait_until_idle_session "$MSID"
cli session set "$MSID" --model sonnet >/dev/null
OUT=$(cli send "$MSID" "Reply with the single word SWITCHED." 2>&1)
check "another model takes over on the running agent" '[ "$(q "$MSID" "\"sonnet\" in str([e[\"body\"][\"model\"] for e in events if e[\"body\"][\"type\"] == \"agent_init\"][-1])")" = True ] && [ "$(q "$MSID" "pid()")" = "$MAGENT" ]'
cli session set "$MSID" --model "$MODEL" >/dev/null
OUT=$(cli session set "$MSID" --effort low 2>&1)
check "an effort the running agent takes is saved" 'echo "$OUT" | grep -q "effort=low" && cli session list timing | grep "^${MSID:0:8}" | grep -q "effort=low"'
check "and it's the same process" '[ "$(q "$MSID" "pid()")" = "$MAGENT" ]'
cli send "$MSID" "$(slow_tool 44 midturn midturn.txt)" --no-follow >/dev/null
wait_for "a long tool is running" "$MSID" "'44' in str(events[-5:])"
BEFORE=$SECONDS
cli session set "$MSID" --permission-mode plan >/dev/null
check "a change mid-turn is answered right away" '[ $((SECONDS - BEFORE)) -lt 5 ]'
cli cancel "$MSID" >/dev/null 2>&1
wait_until_idle_session "$MSID"

section "Closing and reopening a chat"
cli session close "$MSID"
check "closing stops its agent" 'wait_gone "$MAGENT"'
check "and not the other chat's" '[ "$(session_status_of "$ASID")" != "" ] && cli session list timing | grep -q closed'
check "a closed chat takes no prompts" '! cli send "$MSID" "hello?" --no-follow >/dev/null 2>&1'
cli session reopen "$MSID"
OUT=$(cli send "$MSID" "What word did I ask you to reply with first in this conversation? Reply with only that word." 2>&1)
check "reopening picks up the conversation" 'echo "$OUT" | grep -q READY'
cli session close "$MSID"

section "Questions"
QSID=$(cli session new timing --model "$MODEL" --permission-mode bypassPermissions)
cli send "$QSID" "Use the AskUserQuestion tool to ask me whether I prefer red or blue (one question, two options, with the labels Red and Blue). Then tell me which I chose in one short sentence." --no-follow >/dev/null
wait_for "a question reaches us, even with full access" "$QSID" "any(e['body']['type'] == 'approval_requested' and e['body']['tool_name'] == 'AskUserQuestion' for e in events)" 120
Q=$(q "$QSID" "[e['body']['input']['questions'][0]['question'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
APPROVAL=$(q "$QSID" "[e['body']['approval_id'] for e in events if e['body']['type'] == 'approval_requested'][-1]")
cli answer "$QSID" "$APPROVAL" "$Q" Blue
wait_for "the agent gets the answer" "$QSID" "'blue' in text().lower()" 120
cli session close "$QSID"

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
env -i HOME="$HOME" USER="$USER" PATH=/usr/bin:/bin ROUNDHOUSE_HOME="$ROUNDHOUSE_HOME" ROUNDHOUSE_WORKSPACES="$ROUNDHOUSE_WORKSPACES" \
  "$BIN/roundhoused" hub >>"$WORK/finder-hub.log" 2>&1 &
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
q "$MSID" "'  session 3, with one Sonnet reply and a plan: \${:.4f}'.format(sum(e['body'].get('cost_usd') or 0 for e in events if e['body']['type'] == 'turn_ended'))"
q "$QSID" "'  session 4: \${:.4f}'.format(sum(e['body'].get('cost_usd') or 0 for e in events if e['body']['type'] == 'turn_ended'))"

echo
echo "$passes passed, $failures failed"
[ "$failures" -eq 0 ]
