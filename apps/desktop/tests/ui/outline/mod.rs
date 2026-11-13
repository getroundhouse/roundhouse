//! The outline at the right of a chat: a tick for each prompt and a dot for each compaction, and the list that opens
//! over them.

#![cfg(test)]

mod jumps;

mod navigating;

use navigating::{
    compact, open_the_list, prompt_rows, row_id, scrolled_to, scrolled_to_exactly, selected, wait_out_the_list,
};
