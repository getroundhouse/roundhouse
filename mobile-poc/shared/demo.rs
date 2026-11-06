//! The screen under test. Both mobile paths include this file so the comparison uses the same UI.
//!
//! It exercises what the real transcript needs: a variable-height virtualized list with thousands
//! of items, Markdown with fenced code, a composer, a button and sidebar rows. It also keeps a
//! small event log, because a screenshot cannot show whether an event was delivered.

use std::{fs::OpenOptions, io::Write as _, rc::Rc, sync::OnceLock};

use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _, StyledExt as _, Theme, ThemeMode,
    bubble::Bubble,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    list::ListItem,
    message_scroller::{MessageScroller, MessageScrollerState},
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Path B has to ask for the keyboard itself when an input gains or loses focus. Path A leaves
/// this unset, because `gpui_ios` follows focus on its own.
pub static KEYBOARD_HOOK: OnceLock<fn(bool)> = OnceLock::new();

fn keyboard_focus_changed(focused: bool) {
    if let Some(hook) = KEYBOARD_HOOK.get() {
        hook(focused);
    }
}

static MONO_FONT: OnceLock<Option<&'static str>> = OnceLock::new();

/// Picks the monospace family. Each host calls this with its platform's text system before
/// starting the app.
///
/// The kit's platform default names a Linux font on iOS and silently lands on the proportional
/// system font, so the theme needs an explicit family. Listing families is not enough to choose
/// one: on the iOS 26.5 simulator they are all listed, but GPUI rejects every one except the system
/// font when it loads them, and then panics at layout. So try to load each candidate here, through
/// the platform, because GPUI's own `resolve_font` panics instead of returning an error.
pub fn probe_mono_font(text_system: &dyn PlatformTextSystem) {
    let mono = ["Menlo", "Courier New"]
        .into_iter()
        .find(|family| text_system.font_id(&font(*family)).is_ok());
    log_event(format!("monospace family: {mono:?}"));
    let _ = MONO_FONT.set(mono);
}

/// Runs once after the window opens. Path A uses it to try native views on top of GPUI.
pub static ON_WINDOW_OPEN: OnceLock<fn(&mut Window)> = OnceLock::new();

pub const ITEM_COUNT: usize = 5_000;
pub const WORKSPACES: [&str; 5] = ["yaounde", "lisbon", "nairobi", "osaka", "quito"];

/// Appends one line to `$HOME/Documents/poc.log`, inside the app's data container.
pub fn log_event(line: impl AsRef<str>) {
    let Ok(home) = std::env::var("HOME") else { return };
    let dir = format!("{home}/Documents");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(format!("{dir}/poc.log")) {
        let _ = writeln!(file, "{}", line.as_ref());
    }
}

#[derive(Clone)]
enum Item {
    User(SharedString),
    Assistant(SharedString),
    Tool { title: SharedString, output: SharedString },
}

const RUST_BLOCK: &str = "```rust\nfn follow_tail(list: &ListState, rows: usize) -> Result<()> {\n    // Keep the newest row in view while streaming.\n    let last = rows.saturating_sub(1);\n    list.scroll_to_reveal_item(last);\n    println!(\"following row {last}\");\n    Ok(())\n}\n```";
const TS_BLOCK: &str = "```ts\nexport async function resume(id: string): Promise<Session> {\n  const events = await client.range(id, { from: 0, limit: 200 });\n  return events.reduce(apply, new Session(id));\n}\n```";
const BASH_BLOCK: &str = "```bash\ncargo test -p daemon --release -- --nocapture\ngit worktree add ../yaounde -b yaounde/mobile\n```";

/// Deterministic transcript with a deliberately wide spread of row heights.
fn generate(count: usize) -> Vec<Item> {
    (0..count)
        .map(|ix| {
            let words = "the daemon keeps an ordered event log so a client only reads the range it needs ";
            match ix % 7 {
                0 => Item::User(format!("Message {ix}: can you check why the last run stalled?").into()),
                1 => Item::Assistant(
                    format!(
                        "## Finding {ix}\n\nThe stall comes from **`flush_batch`**, which holds the lock across an *await*. See [the docs](https://example.com).\n\n{RUST_BLOCK}\n\nAfter this change the writer never blocks the reader."
                    )
                    .into(),
                ),
                2 => Item::Tool {
                    title: format!("Bash  #{ix}").into(),
                    output: (0..1 + ix % 9)
                        .map(|line| format!("test transcript::row_{line} ... ok"))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into(),
                },
                3 => Item::Assistant(format!("{ix}: {}", words.repeat(1 + ix % 12)).into()),
                4 => Item::Assistant(
                    format!("### Plan {ix}\n\n1. Read the range.\n2. Apply each event:\n\n{TS_BLOCK}\n\n- Keep raw events\n- Rebuild views from them\n\n> Clients never hold a whole transcript.").into(),
                ),
                5 => Item::Assistant(format!("Done with step {ix}. Run this next:\n\n{BASH_BLOCK}").into()),
                _ => Item::Assistant(format!("Short reply {ix}.").into()),
            }
        })
        .collect()
}

pub struct DemoApp {
    items: Rc<Vec<Item>>,
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    filter: Entity<InputState>,
    drawer_open: bool,
    active_workspace: usize,
    taps: usize,
    _subscriptions: Vec<Subscription>,
}

impl DemoApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let items = Rc::new(generate(ITEM_COUNT));
        let scroller = cx.new(|cx| MessageScrollerState::new(items.len(), cx));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 5)
                .submit_on_enter(true)
                .input_action(TextInputAction::Send)
                .placeholder("Message the agent")
        });
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter workspaces"));

        let subscriptions = vec![
            cx.observe(&scroller, |_, _, cx| cx.notify()),
            cx.subscribe(&composer, |this, state, event: &InputEvent, cx| {
                match event {
                    InputEvent::Change => log_event(format!("composer change: {:?}", state.read(cx).value())),
                    InputEvent::PressEnter { secondary, shift } => {
                        log_event(format!("composer PressEnter secondary={secondary} shift={shift}"));
                        this.pending_send(cx);
                    }
                    InputEvent::Focus => {
                        log_event("composer focus");
                        keyboard_focus_changed(true);
                    }
                    InputEvent::Blur => {
                        log_event("composer blur");
                        keyboard_focus_changed(false);
                    }
                }
                cx.notify();
            }),
            cx.subscribe(&filter, |_, state, event: &InputEvent, cx| match event {
                InputEvent::Change => log_event(format!("filter change: {:?}", state.read(cx).value())),
                InputEvent::PressEnter { .. } => log_event("filter PressEnter"),
                InputEvent::Focus => keyboard_focus_changed(true),
                InputEvent::Blur => keyboard_focus_changed(false),
            }),
            // Records whether GPUI saw a key event at all, whatever the focused element does with it.
            cx.intercept_keystrokes(|event, _, _| {
                log_event(format!("keystroke: {:?}", event.keystroke));
            }),
        ];

        let mut this = Self {
            items,
            scroller,
            composer,
            filter,
            drawer_open: false,
            active_workspace: 0,
            taps: 0,
            _subscriptions: subscriptions,
        };
        this.apply_scene(window, cx);
        this
    }

    /// Launch-time state so each screen can be photographed without touch input.
    /// Set with `SIMCTL_CHILD_POC_SCENE=<scene> xcrun simctl launch ...`.
    fn apply_scene(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let scene = std::env::var("POC_SCENE").unwrap_or_default();
        log_event(format!("launch scene={scene:?}"));
        match scene.as_str() {
            "top" => self.scroll_to(0, cx),
            "mid" => self.scroll_to(2501, cx),
            "drawer" => self.drawer_open = true,
            "focus" => self.composer.update(cx, |state, cx| state.focus(window, cx)),
            _ => {}
        }
    }

    fn scroll_to(&mut self, index: usize, cx: &mut Context<Self>) {
        self.scroller.update(cx, |state, cx| {
            state.scroll_to_item(index, cx);
        });
    }

    fn pending_send(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value();
        log_event(format!("send requested with {} bytes", text.len()));
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value();
        log_event(format!("send button: {text:?}"));
        if text.trim().is_empty() {
            return;
        }
        Rc::make_mut(&mut self.items).push(Item::User(text.trim().to_string().into()));
        self.scroller.update(cx, |state, cx| {
            state.append(1, cx);
            state.scroll_to_end(cx);
        });
        self.composer.update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn header(&self, top_index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("header")
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("toggle-drawer")
                    .icon(IconName::PanelLeftOpen)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.taps += 1;
                        this.drawer_open = !this.drawer_open;
                        log_event(format!("drawer button tapped, open={}", this.drawer_open));
                        cx.notify();
                    })),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().font_semibold().truncate().child(WORKSPACES[self.active_workspace]))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{} items, top row #{top_index}, taps {}", self.items.len(), self.taps)),
                    ),
            )
            .child(
                Button::new("to-top").label("Top").small().outline().on_click(cx.listener(|this, _, _, cx| {
                    this.taps += 1;
                    log_event("top button tapped");
                    this.scroll_to(0, cx);
                    cx.notify();
                })),
            )
    }

    fn composer(&self, bottom: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("composer")
            .w_full()
            .flex_shrink_0()
            .items_end()
            .gap_2()
            .px_3()
            .pt_2()
            .pb(bottom + px(8.))
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().min_w_0().child(Textarea::new(&self.composer)))
            .child(
                Button::new("send").icon(IconName::ArrowUp).primary().on_click(cx.listener(|this, _, window, cx| {
                    this.taps += 1;
                    this.send(window, cx);
                })),
            )
    }

    fn drawer(&self, insets: &WindowInsets, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = SidebarMenu::new().children(WORKSPACES.iter().enumerate().map(|(ix, name)| {
            SidebarMenuItem::new(*name)
                .icon(IconName::Folder)
                .active(ix == self.active_workspace)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.taps += 1;
                    this.active_workspace = ix;
                    this.drawer_open = false;
                    log_event(format!("sidebar row tapped: {ix}"));
                    cx.notify();
                }))
        }));
        let footer_row = ListItem::new("list-item-row")
            .py_2()
            .child(h_flex().gap_2().child("ListItem row"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.taps += 1;
                log_event("list item tapped");
                cx.notify();
            }));

        div()
            .id("drawer-layer")
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("drawer-scrim")
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black().opacity(0.5))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.drawer_open = false;
                        log_event("scrim tapped");
                        cx.notify();
                    })),
            )
            .child(
                v_flex()
                    .id("drawer-panel")
                    // Without this a tap on a row also reaches the scrim behind it.
                    .occlude()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(300.))
                    .bg(cx.theme().sidebar)
                    .pt(insets.safe_area.top)
                    .pb(insets.safe_area.bottom)
                    .child(div().p_3().flex_shrink_0().child(Input::new(&self.filter).small()))
                    .child(
                        Sidebar::new("workspaces")
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .collapsible(false)
                            .child(SidebarGroup::new("Workspaces").child(menu)),
                    )
                    .child(div().p_3().flex_shrink_0().child(footer_row)),
            )
    }
}

fn render_row(index: usize, item: &Item, cx: &App) -> AnyElement {
    match item {
        Item::User(text) => h_flex()
            .id(("row", index))
            .w_full()
            .justify_end()
            .child(div().max_w(relative(0.85)).child(Bubble::new().child(text.clone())))
            .into_any_element(),
        Item::Assistant(text) => div()
            .id(("row", index))
            .w_full()
            .min_w_0()
            .child(TextView::markdown(("md", index), text.clone()).w_full().min_w_0().scrollable(false))
            .into_any_element(),
        Item::Tool { title, output } => v_flex()
            .id(("row", index))
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted)
            .p_2()
            .gap_1()
            .child(div().text_xs().font_semibold().child(title.clone()))
            .child(div().text_xs().font_family(cx.theme().mono_font_family.clone()).child(output.clone()))
            .into_any_element(),
    }
}

impl Render for DemoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let insets = window.insets();
        // The keyboard covers the home indicator area, so use whichever is taller, not the sum.
        let bottom = insets.effective().bottom;
        let top_index = self.scroller.read(cx).logical_scroll_top().item_ix;
        let items = self.items.clone();

        div()
            .id("root")
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                v_flex()
                    .size_full()
                    .pt(insets.safe_area.top)
                    .child(self.header(top_index, cx))
                    .child(
                        div().flex_1().min_h_0().w_full().child(
                            MessageScroller::new("transcript", self.scroller.clone(), move |index, _window, cx| {
                                match items.get(index) {
                                    Some(item) => render_row(index, item, cx),
                                    None => div().into_any_element(),
                                }
                            })
                            .size_full(),
                        ),
                    )
                    .child(self.composer(bottom, cx)),
            )
            .when(self.drawer_open, |this| this.child(self.drawer(&insets, cx)))
    }
}

/// Call once inside the application's run callback, after `gpui_kit::init`.
pub fn open(cx: &mut App) {
    let options = WindowOptions::default();
    gpui_kit::open_window(options, cx, |window, cx| {
        Theme::change(ThemeMode::Dark, Some(window), cx);
        if let Some(Some(mono)) = MONO_FONT.get() {
            Theme::update(cx, |theme| theme.mono_font_family = (*mono).into());
        }
        if let Some(hook) = ON_WINDOW_OPEN.get() {
            hook(window);
        }
        cx.new(|cx| DemoApp::new(window, cx))
    })
    .expect("open window");
    cx.activate(true);
}
