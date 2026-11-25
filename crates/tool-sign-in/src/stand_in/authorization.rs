use std::collections::HashMap;

use url::Url;

use super::script::{Iss, Person, Script};
use super::serving::Addresses;
use super::tokens::{Approved, Tokens};

/// Has the person do on the page at `page` what `script` says, and gives the query of where they're sent back to.
/// Panics where the page wasn't asked for as an authorization server has it, as one shows an error on.
pub(super) fn visited(page: &Url, script: &Script, at: &Addresses, tokens: &mut Tokens) -> String {
    let asked: HashMap<String, String> = page.query_pairs().into_owned().collect();
    assert_eq!(page.path(), "/authorize");
    assert_eq!(asked["tenant"], "made-up", "what its page's address had in it is kept");
    assert_eq!(asked["response_type"], "code");
    assert_eq!(asked["code_challenge_method"], "S256");
    assert_eq!(asked["resource"], at.resource(script));
    let mut back = Url::parse(&asked["redirect_uri"]).unwrap();
    let iss = match script.iss {
        Iss::Said | Iss::SaidUnannounced => Some(at.issuer(script)),
        Iss::Unsaid | Iss::Missing => None,
        Iss::Another | Iss::AnotherUnannounced => Some("https://other.example".to_string()),
    };
    match script.person {
        Person::Approves => {
            let code = tokens.approve(Approved {
                challenge: asked["code_challenge"].clone(),
                redirect: asked["redirect_uri"].clone(),
                client: asked["client_id"].clone(),
                resource: asked["resource"].clone(),
            });
            back.query_pairs_mut().append_pair("code", &code);
        }
        Person::Refuses => {
            back.query_pairs_mut().append_pair("error", "access_denied").append_pair("error_description", "made up");
        }
    }
    back.query_pairs_mut().append_pair("state", &asked["state"]).extend_pairs(iss.map(|iss| ("iss", iss)));
    back.query().unwrap().to_string()
}
