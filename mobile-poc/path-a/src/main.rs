//! Path A: gpui-kit on Zed's Apache-2.0 `gpui_ios` backend. The app owns no native code.

#[cfg(target_os = "ios")]
#[path = "../../shared/demo.rs"]
mod demo;

/// Experiment: can a native view share the screen on path A? `gpui_ios` owns the app and scene
/// delegates, so GPUI cannot go inside a native screen, but the window handle exposes its UIView.
/// Set `POC_HOST=native` to add a UITextField on top of it.
#[cfg(target_os = "ios")]
fn add_native_text_field(window: &mut gpui_kit::Window) {
    use objc2::{class, ffi::NSInteger, msg_send, runtime::AnyObject};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

    if std::env::var("POC_HOST").as_deref() != Ok("native") {
        return;
    }
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else { return };
    let view: *mut AnyObject = handle.ui_view.as_ptr().cast();
    let frame = NSRect::new(NSPoint::new(12., 640.), NSSize::new(378., 44.));
    let placeholder = NSString::from_str("Native UITextField");
    // SAFETY: called on the main thread with a live view. UIKit retains the subview.
    unsafe {
        let field: *mut AnyObject = msg_send![class!(UITextField), alloc];
        let field: *mut AnyObject = msg_send![field, initWithFrame: frame];
        let _: () = msg_send![field, setBorderStyle: 3 as NSInteger];
        let _: () = msg_send![field, setPlaceholder: &*placeholder];
        let color: *mut AnyObject = msg_send![class!(UIColor), systemGrayColor];
        let _: () = msg_send![field, setBackgroundColor: color];
        let _: () = msg_send![view, addSubview: field];
    }
}

#[cfg(target_os = "ios")]
struct EventLog;

#[cfg(target_os = "ios")]
impl log::Log for EventLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            demo::log_event(format!("{} {}: {}", record.level(), record.target(), record.args()));
        }
    }

    fn flush(&self) {}
}

#[cfg(target_os = "ios")]
fn main() {
    use gpui_kit::assets::Assets;

    if log::set_logger(&EventLog).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let _ = demo::ON_WINDOW_OPEN.set(add_native_text_field);
    let platform = gpui_ios::current_platform(false);
    demo::probe_mono_font(platform.text_system().as_ref());
    gpui_kit::Application::with_platform(platform)
        .with_assets(Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_ios::ios::set_status_bar_style(gpui_ios::StatusBarContentStyle::Light);
            demo::open(cx);
        });
}

#[cfg(not(target_os = "ios"))]
fn main() {
    eprintln!("Build this for an iOS target; see ../README.md.");
}
