//! How a proposal and its revisions are read from their rows.

use super::super::{At, ByColumn, Column, Columns, StoreError, columns, json_at, rev_of};
use protocol::model::{Proposal, ProposalId, ProposalRevision, Proposed, Response, RevisionNumber, RevisionSummary};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row};

columns! {
    /// A proposal's own columns, from `proposals` as `p`, which a read takes with its newest revision's
    /// [`RevisionColumns`] after them, from [`PROPOSAL_WITH_NEWEST`].
    struct ProposalColumns {
        id = "p.id", workspace_id = "p.workspace_id", session_id = "p.session_id", created_at_ms = "p.created_at_ms",
        rev = "p.rev", withdrawn = "p.withdrawn",
    }
}

columns! {
    /// A revision, from `proposal_revisions` as `r`.
    pub(super) struct RevisionColumns {
        number = "r.number", call = "r.call", proposed = "r.proposed", made_at_ms = "r.made_at_ms",
        response = "r.response", images = "r.images", changed = "r.changed",
    }
}

columns! {
    /// What [`summary_from_row`] reads of a revision.
    struct SummaryColumns { number, call, made_at_ms, response, proposed }
}

/// A proposal with its newest revision, from `proposals` as `p` and `proposal_revisions` as `r`.
pub(super) const PROPOSAL_WITH_NEWEST: &str = "proposals p JOIN proposal_revisions r ON r.proposal_id = p.id
    AND r.number = (SELECT MAX(number) FROM proposal_revisions WHERE proposal_id = p.id)";

/// The response in `column`. One written in a form this build can't read, by a newer one, is still a response, so the
/// revision isn't taken for waiting, and answered again over it.
fn response_at(row: &Row<'_>, column: Column) -> rusqlite::Result<Option<Response>> {
    Ok(row.at::<Option<String>>(column)?.map(|json| serde_json::from_str(&json).unwrap_or(Response::Unknown)))
}

/// What the agent wrote in `column`. Only this build and newer ones write it, and a newer one's fields that this build
/// doesn't know are left out.
fn proposed_at(row: &Row<'_>, column: Column) -> rusqlite::Result<Proposed> {
    json_at(row, column)?
        .ok_or_else(|| rusqlite::Error::InvalidColumnType(column.index(), column.name().into(), Type::Text))
}

/// The revision in a row's columns `c`.
pub(super) fn revision_from_row(row: &Row<'_>, c: RevisionColumns) -> rusqlite::Result<ProposalRevision> {
    let proposed = proposed_at(row, c.proposed)?;
    Ok(ProposalRevision {
        number: RevisionNumber(row.at(c.number)?),
        call: row.at::<Option<String>>(c.call)?.map(Into::into),
        proposed,
        made_at_ms: row.at(c.made_at_ms)?,
        response: response_at(row, c.response)?,
        images: json_at(row, c.images)?.unwrap_or_default(),
        changed: row.at::<Option<String>>(c.changed)?.unwrap_or_default(),
    })
}

/// What a read of a proposal with its newest revision selects, from [`PROPOSAL_WITH_NEWEST`].
pub(super) fn proposal_columns() -> String {
    format!("{}, {}", ProposalColumns::LIST, RevisionColumns::LIST)
}

/// A proposal with its newest revision, as [`proposal_columns`] reads them, without the summary of each, which
/// [`with_summaries`] adds.
pub(super) fn proposal_from_row(row: &Row<'_>) -> rusqlite::Result<Proposal> {
    let c = ProposalColumns::at(0);
    Ok(Proposal {
        id: row.at::<String>(c.id)?.into(),
        workspace_id: row.at::<String>(c.workspace_id)?.into(),
        session_id: row.at::<String>(c.session_id)?.into(),
        current: revision_from_row(row, RevisionColumns::at(ProposalColumns::COUNT))?,
        revisions: Vec::new(),
        withdrawn: json_at(row, c.withdrawn)?,
        created_at_ms: row.at(c.created_at_ms)?,
        rev: rev_of(row, c.rev)?,
    })
}

/// `proposal`, with the summary of each of its revisions, read on `conn`.
pub(super) fn with_summaries(conn: &Connection, mut proposal: Proposal) -> Result<Proposal, StoreError> {
    proposal.revisions = conn
        .prepare_cached(&format!(
            "SELECT {} FROM proposal_revisions WHERE proposal_id = ? ORDER BY number",
            SummaryColumns::LIST
        ))?
        .query_map([proposal.id.as_str()], summary_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(proposal)
}

/// A revision in short, from [`SummaryColumns`]. What the agent wrote, up to 128 KiB a revision, is read only for one
/// the person chose in, whose summary names the options chosen, since SQLite reads a column only when it's asked for,
/// so a record stays cheap to read however often its proposal went another round.
fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<RevisionSummary> {
    let c = SummaryColumns::at(0);
    let response = response_at(row, c.response)?;
    let chosen = match &response {
        Some(Response::Chose(choice)) => proposed_at(row, c.proposed)?.chosen(choice),
        Some(Response::AnotherRound(_) | Response::Unknown) | None => Vec::new(),
    };
    Ok(RevisionSummary {
        number: RevisionNumber(row.at(c.number)?),
        call: row.at::<Option<String>>(c.call)?.map(Into::into),
        made_at_ms: row.at(c.made_at_ms)?,
        response: response.map(|response| response.summary(chosen)),
    })
}

pub(super) fn proposal_on(conn: &Connection, id: &ProposalId) -> Result<Proposal, StoreError> {
    let proposal = conn
        .query_row(
            &format!("SELECT {} FROM {PROPOSAL_WITH_NEWEST} WHERE p.id = ?", proposal_columns()),
            [id.as_str()],
            proposal_from_row,
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound { kind: "proposal", id: id.to_string() })?;
    with_summaries(conn, proposal)
}
