/// Whether a request has been refused since the log last said so, so it says so once rather than at each request,
/// since a CLI can go on asking, as Claude does of a task each second. Its owner makes it again where the next refusal\n/// is to be logged, such as once a request ends.
#[derive(Default)]
pub(super) struct Refusals {
    logged: bool,
}

/// A request that isn't sent. `first` is whether it's the first since its [`Refusals`] was made, which the log says.
pub(super) struct Refused {
    pub(super) first: bool,
}

impl Refusals {
    pub(super) fn refused(&mut self) -> Refused {
        Refused { first: !std::mem::replace(&mut self.logged, true) }
    }
}
