use std::path::Path;

use protocol::worker::ToolsToken;
use serde_json::{Value, json};

use super::*;

fn tools(config: &Path) -> ToolServer {
    ToolServer {
        url: "http://127.0.0.1:50123/mcp".into(),
        token: ToolsToken::new("token-s"),
        config: config.to_path_buf(),
    }
}

#[test]
fn the_cli_is_given_a_file_naming_the_server_and_its_token_which_only_its_user_can_read() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("agents/s-1.mcp.json");

    let args = args(&tools(&config)).unwrap();

    assert_eq!(args, ["--mcp-config".into(), config.clone().into_os_string()]);
    let written: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    let server = json!({
        "type": "http",
        "url": "http://127.0.0.1:50123/mcp",
        "headers": { "Authorization": "Bearer token-s" },
    });
    assert_eq!(written, json!({ "mcpServers": { "roundhouse": server } }));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&config).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn a_file_left_from_before_is_written_over_for_its_user_alone() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("s-1.mcp.json");
    std::fs::write(&config, "an older and much longer file than the one written over it").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    args(&tools(&config)).unwrap();

    let written: Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    assert_eq!(written["mcpServers"]["roundhouse"]["url"], "http://127.0.0.1:50123/mcp");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&config).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn an_agent_whose_file_cant_be_written_goes_without_the_tools() {
    let dir = tempfile::tempdir().unwrap();
    // A file where its folder would be.
    std::fs::write(dir.path().join("agents"), "").unwrap();

    assert_eq!(args(&tools(&dir.path().join("agents/s-1.mcp.json"))), None);
}

#[test]
fn forgetting_removes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("s-1.mcp.json");
    let tools = tools(&config);
    args(&tools).unwrap();

    forget(&config);

    assert!(!config.exists());
}

#[cfg(unix)]
#[test]
fn what_had_a_file_left_from_before_open_never_reads_the_token() {
    use std::io::Read as _;
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("s-1.mcp.json");
    std::fs::write(&config, "").unwrap();
    let mut opened = File::open(&config).unwrap();

    args(&tools(&config)).unwrap();

    let mut read = String::new();
    opened.read_to_string(&mut read).unwrap();
    assert_eq!(read, "", "it still has the file that was there");
}
