//! Writing each event as a line of JSON.

use std::io::Write as _;

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime as _, SystemTime};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// What tracing names an event that isn't named: where it is in the source.
const UNNAMED: &str = "event ";

/// What's put before the fields of an event from the `log` crate, which say where in the source it's from.
const FROM_LOG: &str = "log.";

/// Writes each event as a line of JSON, all of it at once, so lines from several processes appending to one file don't
/// mix.
pub(crate) struct JsonLines<W> {
    to: W,
}

impl<W> JsonLines<W> {
    pub(crate) fn to(to: W) -> Self {
        Self { to }
    }
}

impl<S, W> Layer<S> for JsonLines<W>
where
    S: Subscriber + for<'s> LookupSpan<'s>,
    W: for<'w> MakeWriter<'w> + 'static,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let Some(span) = context.span(id) else { return };
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        span.extensions_mut().insert(fields);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, context: Context<'_, S>) {
        let Some(span) = context.span(id) else { return };
        if let Some(fields) = span.extensions_mut().get_mut::<Fields>() {
            values.record(fields);
        }
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let mut line = line(event, &context).to_string();
        line.push('\n');
        // Nothing can be done about a log that can't be written, least of all logging it.
        let _ = self.to.make_writer_for(event.metadata()).write_all(line.as_bytes());
    }
}

/// `event`'s line: when, its level, the part it's from, its name where it has one, its message, its fields, and the
/// fields of the spans it's in, where an inner span's stand in place of an outer's.
fn line<S>(event: &Event<'_>, context: &Context<'_, S>) -> Value
where
    S: Subscriber + for<'s> LookupSpan<'s>,
{
    let metadata = event.metadata();
    let mut fields = Fields::default();
    event.record(&mut fields);
    let from_log = fields.take_from_log();
    let mut around = Map::new();
    for span in context.event_scope(event).into_iter().flat_map(tracing_subscriber::registry::Scope::from_root) {
        if let Some(span_fields) = span.extensions().get::<Fields>() {
            around.extend(span_fields.0.clone());
        }
    }
    let mut line = Map::new();
    line.insert("time".into(), now().into());
    line.insert("level".into(), metadata.level().as_str().into());
    line.insert("target".into(), from_log.unwrap_or_else(|| metadata.target().into()));
    if !metadata.name().starts_with(UNNAMED) && !metadata.name().starts_with("log event") {
        line.insert("event".into(), metadata.name().into());
    }
    if let Some(message) = fields.0.remove("message") {
        line.insert("message".into(), message);
    }
    if !fields.0.is_empty() {
        line.insert("fields".into(), fields.0.into());
    }
    if !around.is_empty() {
        line.insert("context".into(), around.into());
    }
    line.into()
}

fn now() -> String {
    let mut now = String::new();
    // Writing to a string doesn't fail.
    let _ = SystemTime.format_time(&mut Writer::new(&mut now));
    now
}

/// The fields of an event or a span, as JSON.
#[derive(Default)]
struct Fields(Map<String, Value>);

impl Fields {
    /// Takes out what's there of where an event from the `log` crate is from, and returns the part it's from.
    fn take_from_log(&mut self) -> Option<Value> {
        let target = self.0.remove("log.target");
        self.0.retain(|name, _| !name.starts_with(FROM_LOG));
        target
    }

    fn set(&mut self, field: &Field, value: impl Into<Value>) {
        self.0.insert(field.name().into(), value.into());
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.set(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.set(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.set(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.set(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.set(field, value);
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.set(field, value);
    }
}
