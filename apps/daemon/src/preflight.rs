//! The local provider's single-use preflight command. Its stdin stays open while the hub awaits the answer.

use agents::{AgentOptions, BinaryLookup};
use protocol::capability::settings::ModelCatalog;
use protocol::client::{ApiError, ErrorCode, Outcome};
use protocol::model::AgentKind;
use protocol::preflight::{Asked, Question};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

pub(crate) fn parse_agent(name: &str) -> Result<AgentKind, &'static str> {
    AgentKind::parse(name).ok_or("unknown agent")
}

pub(crate) fn run(
    agent: AgentKind,
    checkout: &Path,
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
    antigravity: Option<PathBuf>,
) -> anyhow::Result<()> {
    let explicit = match agent {
        AgentKind::Claude => claude,
        AgentKind::Codex => codex,
        AgentKind::Antigravity => antigravity,
        AgentKind::Unknown => anyhow::bail!("unknown agent"),
    };
    let binary = BinaryLookup {
        explicit,
        home: std::env::var_os(protocol::paths::HOME_VAR).map(PathBuf::from),
        path: std::env::var_os("PATH"),
    };
    crate::runtime::run_to_end(async {
        let mut input = BufReader::new(tokio::io::stdin());
        let mut line = Vec::new();
        (&mut input).take(protocol::frame::MAX_FRAME_BYTES as u64 + 1).read_until(b'\n', &mut line).await?;
        anyhow::ensure!(line.len() <= protocol::frame::MAX_FRAME_BYTES, "preflight input is too large");
        let asked: Asked = protocol::frame::decode(&line)?;
        let answer = match asked.question {
            _ if asked.version > protocol::preflight::VERSION => {
                Outcome::Error(ApiError::new(ErrorCode::Unsupported, "a newer version of preflight"))
            }
            Question::Models { login } => {
                let options = AgentOptions { binary, login, ..Default::default() };
                tokio::select! {
                    result = models_for(agent, &options, checkout) => Outcome::from(result),
                    // The hub closed the stream, so it no longer wants the answer.
                    _ = input.fill_buf() => return Ok(()),
                }
            }
            Question::Unknown => Outcome::Error(ApiError::new(ErrorCode::Unsupported, "unknown preflight question")),
        };
        let mut out = tokio::io::stdout();
        out.write_all(&protocol::frame::encode(&answer)?).await?;
        out.flush().await?;
        Ok(())
    })?
}

async fn models_for(agent: AgentKind, options: &AgentOptions, cwd: &Path) -> Result<ModelCatalog, ApiError> {
    match agent {
        AgentKind::Codex => Ok(agents::codex::models_for(options, cwd).await?),
        AgentKind::Claude | AgentKind::Antigravity | AgentKind::Unknown => {
            Err(ApiError::new(ErrorCode::Unsupported, "this agent doesn't discover its models"))
        }
    }
}
