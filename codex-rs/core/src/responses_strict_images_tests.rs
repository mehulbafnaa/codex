use super::*;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use pretty_assertions::assert_eq;

const TINY_PNG_BYTES: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2, 0, 0, 5, 0, 1,
    122, 94, 171, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn tiny_png_data_url() -> String {
    format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(TINY_PNG_BYTES)
    )
}

#[test]
fn strict_preparation_strips_detail_from_message_images() {
    let mut items = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: tiny_png_data_url(),
            detail: Some(ImageDetail::High),
        }],
        phase: None,
    }];

    prepare_response_items_for_responses_codex_strict_mode(&mut items)
        .expect("strict image preparation should succeed");

    let ResponseItem::Message { content, .. } = &items[0] else {
        panic!("expected message item");
    };
    let [ContentItem::InputImage { image_url, detail }] = content.as_slice() else {
        panic!("expected one input image");
    };
    assert!(image_url.starts_with("data:image/png;base64,"));
    assert_eq!(*detail, None);
}

#[test]
fn strict_preparation_accepts_case_insensitive_data_url_markers() {
    let original_image_url = tiny_png_data_url()
        .replacen("data:", "DATA:", 1)
        .replacen(";base64,", ";BASE64,", 1);
    let mut items = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: original_image_url,
            detail: Some(ImageDetail::High),
        }],
        phase: None,
    }];

    prepare_response_items_for_responses_codex_strict_mode(&mut items)
        .expect("strict image preparation should accept data URL markers case-insensitively");

    let ResponseItem::Message { content, .. } = &items[0] else {
        panic!("expected message item");
    };
    let [ContentItem::InputImage { image_url, detail }] = content.as_slice() else {
        panic!("expected one input image");
    };
    assert!(image_url.starts_with("data:image/png;base64,"));
    assert_eq!(*detail, None);
}

#[test]
fn strict_preparation_detail_modes_match_responses_defaults() {
    assert_eq!(
        prompt_image_mode_for_responses_codex_strict_detail(/*detail*/ None)
            .expect("missing detail should default to original"),
        PromptImageMode::ResponsesLiteOriginal
    );
    assert_eq!(
        prompt_image_mode_for_responses_codex_strict_detail(Some(ImageDetail::Auto))
            .expect("auto detail should use original"),
        PromptImageMode::ResponsesLiteOriginal
    );
    assert_eq!(
        prompt_image_mode_for_responses_codex_strict_detail(Some(ImageDetail::High))
            .expect("high detail should use high"),
        PromptImageMode::ResizeToFit
    );
    assert_eq!(
        prompt_image_mode_for_responses_codex_strict_detail(Some(ImageDetail::Original))
            .expect("original detail should use original"),
        PromptImageMode::ResponsesLiteOriginal
    );
}

#[test]
fn strict_preparation_rejects_http_images() {
    let mut items = vec![ResponseItem::FunctionCallOutput {
        call_id: "call-1".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::InputImage {
                image_url: "https://example.com/image.png".to_string(),
                detail: Some(ImageDetail::Original),
            },
        ]),
    }];

    let err = prepare_response_items_for_responses_codex_strict_mode(&mut items)
        .expect_err("HTTP image URL should fail");

    assert!(matches!(
        err,
        ResponsesCodexStrictImagePreparationError::NonDataUrl { .. }
    ));
}

#[test]
fn strict_preparation_request_fallback_skips_prepared_images() {
    let original_image_url = tiny_png_data_url();
    let mut items = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: original_image_url.clone(),
            detail: None,
        }],
        phase: None,
    }];

    prepare_response_items_for_responses_codex_strict_mode_request_fallback(&mut items)
        .expect("request fallback should succeed for already-prepared image");

    let ResponseItem::Message { content, .. } = &items[0] else {
        panic!("expected message item");
    };
    let [ContentItem::InputImage { image_url, detail }] = content.as_slice() else {
        panic!("expected one input image");
    };
    assert_eq!(image_url, &original_image_url);
    assert_eq!(*detail, None);
}

#[test]
fn strict_preparation_request_fallback_rejects_http_images_without_detail() {
    let mut items = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: "https://example.com/image.png".to_string(),
            detail: None,
        }],
        phase: None,
    }];

    let err = prepare_response_items_for_responses_codex_strict_mode_request_fallback(&mut items)
        .expect_err("unprepared HTTP image URL should fail even without detail metadata");

    assert!(matches!(
        err,
        ResponsesCodexStrictImagePreparationError::NonDataUrl { .. }
    ));
}

#[test]
fn strict_preparation_rejects_low_detail() {
    let mut items = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: tiny_png_data_url(),
            detail: Some(ImageDetail::Low),
        }],
        phase: None,
    }];

    let err = prepare_response_items_for_responses_codex_strict_mode(&mut items)
        .expect_err("low detail should fail");

    assert!(matches!(
        err,
        ResponsesCodexStrictImagePreparationError::UnsupportedLowDetail
    ));
}

#[test]
fn strict_preparation_rejects_unsupported_url_schemes() {
    let mut items = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image_url: "file:///tmp/image.png".to_string(),
            detail: None,
        }],
        phase: None,
    }];

    let err = prepare_response_items_for_responses_codex_strict_mode(&mut items)
        .expect_err("unsupported scheme should fail");

    assert!(matches!(
        err,
        ResponsesCodexStrictImagePreparationError::NonDataUrl { .. }
    ));
}
