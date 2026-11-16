pub(super) const DEVICES: &str = r"CREATE TABLE devices (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        token_sha256 BLOB NOT NULL UNIQUE,
        paired_at_ms INTEGER NOT NULL,
        last_seen_ms INTEGER NOT NULL
    );
";
