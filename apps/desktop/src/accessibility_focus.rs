//! Tells macOS accessibility which element has keyboard focus, which GPUI leaves out. Dictation apps such as Spokenly
//! ask it before they paste, and won't paste unless a text box has focus, and a screen reader asks it to follow focus.
//!
//! GPUI builds an accessibility tree with AccessKit, and hangs it on the window's content view. GPUI's own view, which
//! has keyboard focus, sits inside the content view, and macOS asks the view with keyboard focus what has focus. With no
//! answer of its own, that view names the whole window, so the composer never looks focused, even though AccessKit
//! knows it is. Zed's GPUI does the same, so upgrading won't fix it. This gives GPUI's view the answer AccessKit gives on
//! its parent, until GPUI hangs AccessKit on its own view.

use std::ptr::null_mut;
use std::sync::OnceLock;

use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{class, msg_send, sel};

/// GPUI's view, which `gpui_macos` registers as it loads.
const GPUI_VIEW: &std::ffi::CStr = c"GPUIView";

/// What installing found in GPUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Installed {
    /// GPUI's view now says what AccessKit says has focus.
    Added,
    /// GPUI's view has an answer of its own, so GPUI may have fixed this, and this module can go.
    GpuiAnswers,
    /// GPUI has no view by that name any more, so this does nothing.
    NoGpuiView,
}

/// Has GPUI's view answer what has focus with what AccessKit says. It changes the class rather than a window, so it
/// holds for every window, and only the first call does anything. Later calls return what that one found.
pub(crate) fn install() -> Installed {
    static INSTALLED: OnceLock<Installed> = OnceLock::new();
    *INSTALLED.get_or_init(|| match AnyClass::get(GPUI_VIEW) {
        Some(view) if answer_from_parent(view) => Installed::Added,
        Some(_) => Installed::GpuiAnswers,
        None => Installed::NoGpuiView,
    })
}

/// Gives `view`, a class of `NSView`, an `accessibilityFocusedUIElement` that answers with its parent's. Returns false,
/// and changes nothing, if the class already has one of its own.
fn answer_from_parent(view: &AnyClass) -> bool {
    let method: extern "C-unwind" fn(&AnyObject, Sel) -> *mut AnyObject = focused_element;
    // SAFETY: `Imp` is the type-erased form the runtime keeps every method in, and it calls this one as the encoding
    // below says: with `self` and `_cmd`, returning an object, which is the signature it has.
    let method = unsafe { std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel) -> *mut AnyObject, Imp>(method) };
    let class = std::ptr::from_ref(view).cast_mut();
    // SAFETY: `class` is a registered class. Adding a method to one is allowed at any time, and the runtime only adds
    // it if the class doesn't implement the selector itself. The encoding matches `focused_element`'s signature.
    unsafe { class_addMethod(class, sel!(accessibilityFocusedUIElement), method, c"@@:".as_ptr()) }.as_bool()
}

/// What the view's parent, which carries AccessKit's tree, says has focus. AccessKit says nothing until its tree is
/// built, and then the view answers as it did before, with the window.
extern "C-unwind" fn focused_element(this: &AnyObject, _: Sel) -> *mut AnyObject {
    // SAFETY: `this` is an `NSView`, and `superview` takes nothing and returns a view, or nil.
    let parent: *mut AnyObject = unsafe { msg_send![this, superview] };
    let answer: *mut AnyObject = if parent.is_null() {
        null_mut()
    } else {
        // SAFETY: `parent` is a live `NSView`, and this takes nothing and returns an element, or nil, autoreleased.
        unsafe { msg_send![parent, accessibilityFocusedUIElement] }
    };
    if answer.is_null() {
        // SAFETY: `this` is an `NSView`, so `NSView`'s own answer applies to it. It takes nothing and returns an
        // element, or nil, autoreleased.
        unsafe { msg_send![super(this, class!(NSView)), accessibilityFocusedUIElement] }
    } else {
        answer
    }
}

#[cfg(test)]
mod tests {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, Sel};
    use objc2::{class, msg_send, sel};

    use super::{Installed, answer_from_parent, install};

    #[test]
    fn gpui_has_a_view_that_takes_the_answer() {
        assert_eq!(install(), Installed::Added, "GPUI renamed its view, or answers for itself, so this module can go");
    }

    #[test]
    fn a_view_says_what_its_parent_says_has_focus() {
        let parent = view_of(parent_naming_itself());
        let child = view_of(child_class("RoundhouseTestAnsweringChild"));
        // SAFETY: both are live views, and `addSubview:` takes one and returns nothing.
        let () = unsafe { msg_send![&*parent, addSubview: &*child] };

        // SAFETY: `child` is a live view, and this takes nothing and returns an element, or nil.
        let focused: *mut AnyObject = unsafe { msg_send![&*child, accessibilityFocusedUIElement] };

        assert_eq!(focused, Retained::as_ptr(&parent).cast_mut());
    }

    #[test]
    fn a_class_that_answers_for_itself_is_left_alone() {
        let class = parent_naming_itself();

        assert!(!answer_from_parent(class));
    }

    /// A view class with the answer added, as GPUI's view has.
    fn child_class(name: &str) -> &'static AnyClass {
        let class = ClassBuilder::new(&std::ffi::CString::new(name).unwrap(), class!(NSView)).unwrap().register();
        assert!(answer_from_parent(class));
        class
    }

    /// A view class that says it has focus itself, as AccessKit's content view says what in its tree does.
    fn parent_naming_itself() -> &'static AnyClass {
        static CLASS: std::sync::OnceLock<&'static AnyClass> = std::sync::OnceLock::new();
        CLASS.get_or_init(|| {
            extern "C-unwind" fn itself(this: &AnyObject, _: Sel) -> *mut AnyObject {
                std::ptr::from_ref(this).cast_mut()
            }
            let mut builder = ClassBuilder::new(c"RoundhouseTestFocusedParent", class!(NSView)).unwrap();
            let method = itself as extern "C-unwind" fn(_, _) -> _;
            // SAFETY: the method's signature matches the selector's: no arguments, and an object returned.
            unsafe { builder.add_method(sel!(accessibilityFocusedUIElement), method) };
            builder.register()
        })
    }

    fn view_of(class: &AnyClass) -> Retained<AnyObject> {
        // SAFETY: `class` is a class of `NSView`, and `new` makes one, retained.
        unsafe { msg_send![class, new] }
    }
}
