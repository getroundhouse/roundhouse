//! What each row of the log looks like.

mod alone;
mod around;
mod asked;
mod cache_rule;
mod column;
pub(crate) mod compaction;
mod failed_request;
mod line;
mod message;
mod mode;
mod notices;
mod own_turn;
mod plan_card;
mod prompt;
mod shown;
mod tail;
mod text;
mod thinking;
mod tools;
mod turn_end;
mod turn_times;
mod update_rule;

pub(crate) use alone::rows_repeating_report;
pub use alone::subagent_standing;
pub(crate) use around::{NEAR, REACH, SubagentCalls, blocks_of, redrawn_with, retry_at};
use column::column;
pub(crate) use column::room_under;
pub use column::{ABOVE_THE_COMPOSER, around_column, in_column};
pub(in crate::rows) use message::InRow;
use message::render_block;
use notices::exit_output;
use plan_card::plan_card;
pub(crate) use shown::{Found, of_row as found_in_row, rows_drawing, rows_to_count};
use text::{from_blocks, written};
pub use tools::proposal::{ResponseLine as ProposalResponseLine, response_line as proposal_response_line};
pub(crate) use tools::{ChangeAsked, agent::Opens};

mod rendering;
pub(crate) use rendering::{Item, RULE_ICON, Shared, list_row_of, render_row};
use rendering::{colored_line, empty, marked_line, muted_line, render_event, row, rule_across, rule_middle, rule_row};
