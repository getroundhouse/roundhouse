//! Path B: gpui-kit on Longbridge's `gpui-pre-mobile`, hosted by a Swift UIKit app.

#[cfg(target_os = "ios")]
#[path = "../../shared/demo.rs"]
mod demo;

#[cfg(target_os = "ios")]
mod ios {
    use std::{cell::RefCell, rc::Rc};

    use gpui_kit::{Application, ApplicationHandle, Platform as _, assets::Assets};
    use gpui_mobile::ios::{IosPlatform, ffi};

    thread_local! {
        // UIKit owns the run loop, so GPUI has to outlive this function.
        static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
    }

    /// Starts GPUI and opens the demo window. The fork's own `ffi::run_app` builds the
    /// `Application` privately with no way to install assets, and the kit's icons need them,
    /// so this does the same steps through its public entry points.
    #[unsafe(no_mangle)]
    pub extern "C" fn poc_start() {
        let _ = super::demo::KEYBOARD_HOOK.set(|show| {
            if show {
                gpui_mobile::show_keyboard();
            } else {
                gpui_mobile::hide_keyboard();
            }
        });
        ffi::gpui_ios_initialize();
        let platform = Rc::new(IosPlatform::new());
        super::demo::probe_mono_font(platform.text_system().as_ref());
        let application = Application::with_platform(platform)
            .with_assets(Assets)
            .run_embedded(|cx| {
                gpui_kit::init(cx);
                super::demo::open(cx);
            });
        APPLICATION.with(|slot| *slot.borrow_mut() = Some(application));
        ffi::gpui_ios_did_finish_launching(std::ptr::null_mut());
    }
}
