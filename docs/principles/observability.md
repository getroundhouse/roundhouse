# Observability

These rules say what Roundhouse records about its own running: how each part logs, what a log may hold, and where logs go. Observability is logs, metrics and traces. Logs are what's built on now, and metrics and traces come later.

## Logs

- **Every process logs with `tracing`, set up by the `logging` crate,** so a hub, a worker and the desktop app write the same lines at the same levels. A process's `main` gives it where to write and the levels, and nothing else sets up a log. *Not built:* the iPhone app and the command line.
- **A log is a line of JSON for each event,** so a program or an agent can search it by field. A line has the time, level, target, event name, message, the event's fields, and the context it happened in. A hub or a worker run from a terminal writes text there instead, and the app writes text to stderr as well.
- **A reader of a log skips a line that isn't JSON,** since a log can hold some: what a process prints before its log starts or as it panics, and what a worker of an older build wrote.
- **An event has a name, a fixed message and its values as fields:** `warn!(name: "github.read.failed", %workspace_id, %error, "Couldn't read pull requests")`. The name says what happened and doesn't change when the wording does, and a value is never in the message. A policy test holds every crate to it.
- **Context that many lines share is on a `tracing` span around the work,** such as the workspace's and the session's IDs, so each line has it. That's all a `tracing` span is for, since traces use OpenTelemetry's own. *Not built:* the workspace's and the session's on the lines of a client's request, which have the request's.
- **The level says who should care.**
  - `error`: something is lost or stays broken until a person acts.
  - `warn`: something failed that shouldn't have, and the product went on without it.
  - `info`: something happened once that explains what came after, such as a worker starting.
  - `debug`: a failure the product expects, or a step it takes often.
- **Each part logs its own code at `info`, and what it's built on at `warn`.** `ROUNDHOUSE_LOG` changes both.

## What a log may hold

- **No log holds what a person or an agent wrote or said,** at any level: a prompt, a reply, a file's contents, what was dictated, or the words it was hinted.
- **No log holds a secret,** such as a token. A type that holds one has a `Debug` that leaves it out. A log may name an account, by its name or its email address, since it stays on the machine it's about unless someone sends it on.
- **A log may hold a path,** since a failure to read a file says little without it.
- **A panic is logged with its message, where it happened and a backtrace,** since the message is often the quickest way to see what went wrong. It's the one exception to the first rule: a message can quote what the code was working on, as slicing a string where it can't be sliced quotes the string, and it's exported with the rest once exporting is set up.

## Where logs go

- **A log is a file on the machine it's about, in the data directory, that only its user can read:** `hub.log`, `app.log`, and a file in `worker-logs/` for each worker. Whoever opens the file gives it to the process, so a worker's is its stderr, kept by its hub wherever it runs.
- **Whoever opens a log for a process it starts moves it aside first when it's over 10 MB, keeping one from before,** so a log doesn't grow without end. A process writes to one file for as long as it runs. *Not built:* for a hub run as a service, whose log its service manager opens.
- **Nothing is sent anywhere unless someone sets it up.** With OpenTelemetry's standard `OTEL_*` variables set, a hub and its workers also export their logs over OTLP, to where those say. *Not built.*

## Metrics and traces

- **A trace's spans use OpenTelemetry's tracing API, not `tracing`'s,** as its Rust project advises, since it has what a span between processes needs. A log written inside one carries its trace's ID without more code. *Not built.*

## What isn't a log

- **A recording of what someone dictated is a file they chose to keep,** not a log, since it holds what was said. A log line and a recording share an ID, so one can be found from the other. *Not built.*
