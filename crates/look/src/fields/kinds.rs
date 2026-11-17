use crate::named::named;

named! {
    /// How a field is drawn, by where it is. [`crate::field`] makes one of a kind.
    FieldKind {
        /// On its own, with an edge and a faint fill, and a copper ring while it's typed in, such as a dialog's, a
        /// setting's or Settings' search.
        Boxed,
        /// Set in something that frames it already, with no edge, fill or ring of its own, such as Home's search in
        /// its bar or the find bar's.
        Bare,
    }
}

impl FieldKind {
    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
