//! The blocks `MediaRemote` calls back with what it's asked, laid out as Apple's C lays out a block, since this crate
//! uses no crates, `block2` among them.

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicI32, Ordering};

/// How long `MediaRemote` is given to answer, which it does in milliseconds, unless `mediaremoted`, the process that
/// answers for it, is wedged. Long enough for a busy Mac, and short enough that a wedged one doesn't hold dictating's
/// pausing up for long, since each of its steps waits for the one before.
const TIMEOUT_NS: i64 = 2_000_000_000;

/// A block whose answer is waited for: the fields every block has, then the one its function sets. Each is made on
/// the heap and never freed, and marked global, so the runtime neither copies nor frees it, and an answer that comes
/// after its wait has given up still has somewhere to go. This process makes a few, and lasts one dictation.
#[repr(C)]
pub(crate) struct Block {
    isa: *const c_void,
    flags: c_int,
    reserved: c_int,
    /// [`invoke`], for the type of answer this block is given, which the call it's given to decides.
    invoke: *const c_void,
    descriptor: &'static Descriptor,
    answer: AtomicI32,
    answered: Semaphore,
}

/// What every block points to after its function: reserved, then the block's size in bytes.
#[repr(C)]
struct Descriptor {
    reserved: usize,
    size: usize,
}

static DESCRIPTOR: Descriptor = Descriptor { reserved: 0, size: size_of::<Block>() };

/// The flag that marks a block as global, which the runtime never copies or frees.
const BLOCK_IS_GLOBAL: c_int = 1 << 28;

/// Apple's semaphore, from Grand Central Dispatch.
struct Semaphore(*mut c_void);

unsafe extern "C" {
    /// What a global block's `isa` points to, from the blocks runtime in libSystem.
    static _NSConcreteGlobalBlock: c_void;
    fn dispatch_semaphore_create(value: isize) -> *mut c_void;
    fn dispatch_semaphore_signal(semaphore: *mut c_void) -> isize;
    fn dispatch_semaphore_wait(semaphore: *mut c_void, timeout: u64) -> isize;
    fn dispatch_time(when: u64, delta_ns: i64) -> u64;
}

impl Block {
    /// A block whose function takes a `T`, which [`Block::wait`] then gives as an `i32`.
    pub(crate) fn answering<T: Into<i32>>() -> &'static Self {
        let invoke: unsafe extern "C" fn(&Self, T) = invoke::<T>;
        Box::leak(Box::new(Self {
            isa: &raw const _NSConcreteGlobalBlock,
            flags: BLOCK_IS_GLOBAL,
            reserved: 0,
            invoke: invoke as *const c_void,
            descriptor: &DESCRIPTOR,
            answer: AtomicI32::new(0),
            answered: Semaphore::new(),
        }))
    }

    /// The answer, once the block is called, or `None` where it isn't called within [`TIMEOUT_NS`].
    pub(crate) fn wait(&self) -> Option<i32> {
        self.answered.wait().then(|| self.answer.load(Ordering::Acquire))
    }
}

/// A block's function, which `MediaRemote` calls with the block and what it was asked.
unsafe extern "C" fn invoke<T: Into<i32>>(block: &Block, answer: T) {
    block.answer.store(answer.into(), Ordering::Release);
    block.answered.signal();
}

impl Semaphore {
    fn new() -> Self {
        // SAFETY: a semaphore that starts at 0, which is never negative, so it's always made.
        Self(unsafe { dispatch_semaphore_create(0) })
    }

    fn signal(&self) {
        // SAFETY: the semaphore lives as long as its block, which is never freed.
        unsafe { dispatch_semaphore_signal(self.0) };
    }

    /// Whether it was signalled within [`TIMEOUT_NS`].
    fn wait(&self) -> bool {
        // SAFETY: the semaphore lives as long as its block, and `dispatch_time` only adds to now, which is 0.
        unsafe { dispatch_semaphore_wait(self.0, dispatch_time(0, TIMEOUT_NS)) == 0 }
    }
}
