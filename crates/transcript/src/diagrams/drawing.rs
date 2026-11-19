//! Drawing a diagram as SVG, with `merman`.

use super::Palette;
use super::measuring;
use crate::parser_panics::unless_it_panics;
use merman::svg::SvgPipeline;
use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use std::panic::AssertUnwindSafe;
use std::time::Duration;

/// The language a fenced code block is written in for it to be drawn as a diagram.
pub(crate) const LANGUAGE: &str = "mermaid";

/// How long a diagram is given to be drawn, after which it's drawn as its code. A large one, of 500 boxes, takes a
/// tenth of this.
const DEADLINE: Duration = Duration::from_secs(2);

/// The longest a diagram's text can be to be drawn, past which it's drawn as its code: far more than a diagram anyone
/// could read, so the time a text the agent got wrong can take is bounded before it's started.
const LONGEST: usize = 64 * 1024;

/// Whether a code block written in `language` is a diagram, as GitHub reads it.
pub(crate) fn is_diagram(language: Option<&str>) -> bool {
    language.is_some_and(|language| language.eq_ignore_ascii_case(LANGUAGE))
}

/// The diagram `text` describes, as SVG, in `palette`'s colours, or `None` if it can't be drawn: it's a mistake, a
/// kind this doesn't draw, too long, or took too long. Slow, so it's for another thread.
pub(crate) fn svg(text: &str, palette: &Palette) -> Option<Vec<u8>> {
    if text.len() > LONGEST {
        return None;
    }
    let fonts = measuring::fonts()?;
    let request = SvgRequest {
        pipeline: Some(SvgPipeline::resvg_safe()),
        environment: measuring::environment(fonts),
        ..SvgRequest::default()
    };
    let config = merman::SourcePresentationConfig::from_value(palette.config(&fonts.family));
    let renderer = Renderer::new().with_presentation_engine(merman::MermaidEngine::new().with_site_config(config));
    let control = OperationControl::new().with_deadline(DEADLINE);
    // It's not known to panic, but a picture isn't worth the app. Nothing it panics on is used after.
    let drawing = AssertUnwindSafe(|| renderer.render(RenderRequest::svg(text, control, request)));
    // Anything else is no diagram, such as text that names none.
    let RenderOutput::Svg(Some(svg)) = unless_it_panics(drawing)? else { return None };
    Some(svg.svg().as_bytes().to_vec())
}
