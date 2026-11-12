/// Keeps this process's own input and output from what it starts, unless it gives them to a child, as
/// `Stdio::inherit` does.
///
/// On Windows a child gets a copy of every handle its parent can pass on, wanted or not, and a process's own input and
/// output are among them when it was started with them. Everything it starts would hold them, and everything those
/// start in turn, so whatever reads its output to the end would wait for the last of them to exit, not for it. A child
/// given them still gets them, since std makes each child a copy of its own. On Unix what a child is given takes the
/// place of this process's own, so this does nothing there.
///
/// It only reaches what's started after it, since a child started earlier already has its copies.
pub fn keep_stdio_from_children() {
    crate::sys::keep_stdio_from_children();
}
