//! What a run claims of calls.

use crate::capability::Claimed;
use crate::capability::area::area;

area! { "a call";
    /// Each call is recorded typed, as a [`Call`](super::Call), and any tool without a kind of its own as
    /// [`Call::Other`](super::Call::Other) or [`Call::Mcp`](super::Call::Mcp), and its result as the
    /// [`CallOutput`](super::CallOutput) its kind sends back. A call whose input isn't what its kind takes is recorded
    /// as `Other`, never dropped. Without this, every call is recorded as `Other`.
    Typed => typed: Claimed,
    /// A call's input streams as the model writes it, so a long command or edit shows as it forms. What streamed is
    /// shown as it comes, and replaced by the call once it's recorded.
    StreamInput => stream_input: Claimed,
    /// A running call reports what it has done so far: a command's output as it comes, a tool's progress messages, a
    /// patch as it's written. What streamed is shown as it comes, and replaced by the result once it's recorded.
    Progress => progress: Claimed,
    /// What a call changed in each file comes with its result, as hunks with the file's own line numbers, so a client
    /// shows the change rather than working it out from the call.
    FileChanges => file_changes: Claimed,
    /// The result of a call that changed a file says what the file's language server found wrong in it afterwards, as
    /// opencode's does.
    Diagnostics => diagnostics: Claimed,
    /// A command's result says how it exited, as a number, rather than only in its words.
    ExitCodes => exit_codes: Claimed,
    /// A command can run in a terminal of its own, which the agent can write to while it runs.
    Terminals => terminals: Claimed,
    /// The person can run a command in the agent's conversation, without a turn. The hub records the command before
    /// it's sent, and the agent takes or drops it, as it does a prompt. The agent sees the command and what it printed
    /// as if it had run it itself, and the log records it as the person's.
    PersonCommands => person_commands: Claimed,
}
