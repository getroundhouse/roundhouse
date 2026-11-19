use crate::named::named;

named! {
    /// What a chip names, which its mark and what it shows of its name follow. [`crate::chip`] makes one of a kind.
    ChipKind {
        /// A file, named by its path: the icon for its kind, then its name, such as a call's file.
        File,
        /// A folder, named by its path: the folder's icon, then its name, such as where a search looked.
        Folder,
        /// A subagent, by its kind or what it's for: the robot, then its words, such as `Explore`.
        Subagent,
        /// An MCP server a tool is from: a plug, then its name, such as `github`.
        Server,
        /// A site, by its address's host: a globe, then the host, such as what a fetch read.
        Site,
        /// A proposal, by its title: a way that splits in two, for the options it offers, then its title, such as the
        /// one a call made, which opens it in its tab.
        Proposal,
    }
}

impl ChipKind {
    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
