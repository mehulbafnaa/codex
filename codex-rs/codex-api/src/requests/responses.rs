use codex_protocol::models::ResponseItem;
use codex_protocol::models::attach_response_item_ids_to_input;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Compression {
    #[default]
    None,
    Zstd,
}

pub(crate) fn attach_item_ids(payload_json: &mut Value, original_items: &[ResponseItem]) {
    attach_response_item_ids_to_input(payload_json, original_items);
}
