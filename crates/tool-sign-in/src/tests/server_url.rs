use crate::{NotAServerUrl, ServerUrl};

#[test]
fn a_servers_address_is_a_bare_one_over_http_and_reads_back_as_it_was_kept() {
    for text in ["https://tools.example/mcp", "http://127.0.0.1:3000/mcp", " https://tools.example/mcp?a=b "] {
        let server = ServerUrl::parse(text).unwrap();
        assert_eq!(server.as_str(), text.trim());
        let kept = serde_json::to_string(&server).unwrap();
        assert_eq!(serde_json::from_str::<ServerUrl>(&kept).unwrap(), server);
    }
    let others = [
        "",
        "tools.example/mcp",
        "ftp://tools.example",
        "https://",
        "https://ada:pass@tools.example",
        "https://tools.example/#a",
    ];
    for other in others {
        assert_eq!(ServerUrl::parse(other), Err(NotAServerUrl), "{other}");
    }
    assert!(serde_json::from_str::<ServerUrl>(r#""ftp://tools.example""#).is_err());
}
