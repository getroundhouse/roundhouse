/// Declares token types: text that's all it takes to act as whoever it's for, so it's kept out of logs. Its `Debug`
/// says only that it's there, it has no `Display`, and a request or a reply that carries one can be logged whole. It's
/// a plain string on the wire, and in a file of the hub's. A crate that owns one declares it with `protocol::tokens!`, as
/// the hub does its own, which never cross to another process but GitHub.
#[macro_export]
macro_rules! tokens {
    ($($(#[$doc:meta])* $name:ident),+ $(,)?) => {$(
        $(#[$doc])*
        #[derive(::serde::Serialize, ::serde::Deserialize, Clone, PartialEq, Eq, Hash)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(token: impl Into<String>) -> Self {
                Self(token.into())
            }

            /// The token itself, for handing on or keeping, and nothing else.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(concat!(stringify!($name), "(..)"))
            }
        }
    )+};
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::Serialize;

    use crate::capability::conversation::Start;
    use crate::capability::settings::Effort;
    use crate::capability::settings::Settings;
    use crate::client::{ClientRequest, ClientResponse, accounts, devices, github_accounts, github_apps};
    use crate::model::{AgentKind, AgentLogin, GhLogin, GhLoginWorks, GithubToken, LoginToken, ModelName, RunId};
    use crate::pairing::{DeviceToken, Fingerprint, Pairing, PairingCode, PairingLink};
    use crate::worker::{self, GithubCredential, ToolsToken, WorkerRequest, WorkerResponse};

    #[expect(
        unreachable_pub,
        reason = "a token's type is public, as a crate that declares one needs, and this is a test's"
    )]
    mod made_up {
        crate::tokens!(
            /// A token made up for the test.
            MadeUpToken,
        );
    }
    use made_up::MadeUpToken;

    #[test]
    fn a_token_is_its_text_on_the_wire_and_its_name_alone_in_debug_output() {
        let token = MadeUpToken::new("s3cret");

        assert_eq!(serde_json::to_string(&token).unwrap(), r#""s3cret""#);
        assert_eq!(serde_json::from_str::<MadeUpToken>(r#""s3cret""#).unwrap(), token);
        assert_eq!(token.as_str(), "s3cret");
        assert_eq!(format!("{token:?}"), "MadeUpToken(..)");
    }

    const SECRET: &str = "made-up-secret";

    /// What carries the secret, by name, as it's written on the wire and as it's logged.
    struct Carrier {
        name: &'static str,
        wire: String,
        logged: String,
    }

    fn carrier<T: Serialize + Debug>(name: &'static str, value: &T) -> Carrier {
        Carrier { name, wire: serde_json::to_string(value).unwrap(), logged: format!("{value:?}") }
    }

    fn login() -> AgentLogin {
        AgentLogin { token: LoginToken::new(SECRET) }
    }

    fn launch() -> worker::AgentSpec {
        worker::AgentSpec {
            session_id: "s".into(),
            run: RunId::new(1),
            agent: AgentKind::Claude,
            cwd: "/w".into(),
            start: Start::New { conversation: None },
            settings: Settings::default(),
            env: vec![],
            tools_token: Some(ToolsToken::new(SECRET)),
            login: Some(login()),
        }
    }

    fn link() -> PairingLink {
        PairingLink {
            hub_name: "hub".to_string(),
            hosts: vec!["hub.local:7867".to_string()],
            fingerprint: Fingerprint::of_sha256([0; 32]),
            code: PairingCode::from(SECRET),
        }
    }

    /// What a client and a hub send each other that carries a token or a secret, each whole, as a request or a reply
    /// is logged.
    fn between_a_client_and_a_hub() -> Vec<Carrier> {
        let connect = accounts::AccountConnect {
            agent: AgentKind::Claude,
            name: "Work".into(),
            token: LoginToken::new(SECRET),
            account: None,
        };
        let github = github_accounts::GithubAccountConnect { token: GithubToken::new(SECRET), id: None };
        let make = github_apps::GithubAppMake { code: github_apps::GithubManifestCode::new(SECRET) };
        let pair = devices::Pair { code: PairingCode::from(SECRET), device_name: "phone".into() };
        let paired = devices::Paired { device_id: "d".into(), token: DeviceToken::from(SECRET) };
        let pairing = Pairing { link: link(), code: PairingCode::from(SECRET), expires_at_ms: 1 };
        vec![
            carrier("AccountConnect", &ClientRequest::AccountConnect(connect)),
            carrier("GithubAccountConnect", &ClientRequest::GithubAccountConnect(github)),
            carrier("GithubAppMake", &ClientRequest::GithubAppMake(make)),
            carrier("Pair", &ClientRequest::Pair(pair)),
            carrier(
                "Authenticate",
                &ClientRequest::Authenticate(devices::Authenticate { token: DeviceToken::from(SECRET) }),
            ),
            carrier(
                "PairingWait",
                &ClientRequest::PairingWait(devices::PairingWait { code: PairingCode::from(SECRET) }),
            ),
            carrier("Paired", &ClientResponse::Paired(paired)),
            carrier("Pairing", &ClientResponse::Pairing(pairing)),
            Carrier { name: "PairingLink", wire: link().to_string(), logged: format!("{:?}", link()) },
        ]
    }

    /// What a hub and its worker send each other that carries one.
    fn between_a_hub_and_a_worker() -> Vec<Carrier> {
        let title =
            worker::TitleFor { agent: AgentKind::Claude, prompt: "fix".into(), images: vec![], login: Some(login()) };
        let credential = GithubCredential::Account(worker::GitCredential {
            username: "ada".into(),
            token: GithubToken::new(SECRET),
            identity: None,
        });
        let found = worker::GithubLoginReply {
            login: GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes },
            token: Some(worker::FoundToken { token: GithubToken::new(SECRET), renews: true }),
        };
        let settings = worker::previous::Kept {
            model: ModelName::named("opus"),
            permission_mode: crate::fixtures::plan_mode(),
            effort: Effort::Low,
        };
        let previous = worker::previous::AgentStart::of(launch(), settings);
        vec![
            carrier("AgentLogin", &login()),
            carrier("AgentSpec", &WorkerRequest::AgentStart(launch())),
            carrier("protocol 3's AgentStart", &previous.into_request()),
            carrier("TitleFor", &WorkerRequest::TitleFor(title)),
            carrier(
                "AccountFor",
                &WorkerRequest::AccountFor(worker::AccountFor { agent: AgentKind::Claude, login: Some(login()) }),
            ),
            carrier("GitCredential", &credential),
            carrier("GitCredentialSet", &WorkerRequest::GitCredentialSet(worker::GitCredentialSet { credential })),
            carrier("GithubLoginReply", &WorkerResponse::GithubLogin(found)),
        ]
    }

    /// Each holds it as a token's type, so it can't give it away, as one that held it as a `String`, or wrote its own
    /// `Debug`, would.
    #[test]
    fn whatever_carries_a_token_or_a_secret_leaves_it_out_of_debug_output() {
        for Carrier { name, wire, logged } in
            between_a_client_and_a_hub().into_iter().chain(between_a_hub_and_a_worker())
        {
            assert!(wire.contains(SECRET), "{name} doesn't carry the secret, as {wire}");

            assert!(!logged.contains(SECRET), "{name} gives its secret away, as {logged}");
        }
        assert!(format!("{:?}", link()).contains("hub.local:7867"), "the rest of a pairing link is still shown");
    }
}
