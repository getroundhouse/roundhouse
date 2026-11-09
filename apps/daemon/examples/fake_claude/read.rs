//! `read:FILE`: reading an image, the way the real CLI sends it back. The result holds it as an image block, the CLI's
//! own record of the read holds it again, and then comes a line written for the model, saying how it was shrunk.

use protocol::attachment::AttachmentBytes;
use serde_json::json;

use crate::emit;
use crate::turn::Turn;

pub(crate) fn image(turn: &mut Turn, file: &str) {
    let tool_use_id = turn.named_tool_use("Read", json!({"file_path": file}));
    let base64 = AttachmentBytes::new(std::fs::read(file).unwrap()).to_base64();
    let image = json!({"type":"image","source":{"type":"base64","data":base64,"media_type":"image/png"}});
    emit(
        turn.out,
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":tool_use_id,
        "content":[image]}]},"parent_tool_use_id":turn.parent,"session_id":turn.config.session_id,
        "tool_use_result":{"type":"image","file":{"base64":base64,"type":"image/png"}}}),
    );
    let note =
        "[Image: original 2640x1720, displayed at 2000x1303. Multiply coordinates by 1.32 to map to original image.]";
    emit(
        turn.out,
        json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":note}]},
        "parent_tool_use_id":turn.parent,"session_id":turn.config.session_id,"isSynthetic":true}),
    );
}
