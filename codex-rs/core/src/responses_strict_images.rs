use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ImageDetail;
use codex_protocol::models::ResponseItem;
use codex_utils_image::ImageProcessingError;
use codex_utils_image::PromptImageMode;
use codex_utils_image::load_data_url_for_prompt;

const DATA_URL_PREFIX: &str = "data:";

#[derive(Debug, thiserror::Error)]
pub(crate) enum ResponsesCodexStrictImagePreparationError {
    #[error(
        "Responses Codex strict mode image detail only supports `original`, `high`, or `auto`; got `low`"
    )]
    UnsupportedLowDetail,
    #[error(
        "Responses Codex strict mode only supports data URL images; got image_url={image_url_preview:?}"
    )]
    NonDataUrl { image_url_preview: String },
    #[error("Responses Codex strict mode failed to prepare image: {0}")]
    ImageProcessing(#[from] ImageProcessingError),
}

pub(crate) fn prepare_response_items_for_responses_codex_strict_mode(
    items: &mut [ResponseItem],
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    prepare_response_items(items, PreparationMode::AllImages)
}

pub(crate) fn prepare_response_items_for_responses_codex_strict_mode_request_fallback(
    items: &mut [ResponseItem],
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    prepare_response_items(items, PreparationMode::UnpreparedImagesOnly)
}

#[derive(Clone, Copy)]
enum PreparationMode {
    AllImages,
    UnpreparedImagesOnly,
}

fn prepare_response_items(
    items: &mut [ResponseItem],
    mode: PreparationMode,
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    for item in items {
        prepare_response_item(item, mode)?;
    }
    Ok(())
}

fn prepare_response_item(
    item: &mut ResponseItem,
    mode: PreparationMode,
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    match item {
        ResponseItem::Message { content, .. } => prepare_content_items(content, mode),
        ResponseItem::FunctionCallOutput { output, .. }
        | ResponseItem::CustomToolCallOutput { output, .. } => {
            if let Some(content_items) = output.content_items_mut() {
                prepare_function_call_output_content_items(content_items, mode)?;
            }
            Ok(())
        }
        ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::CompactionTrigger
        | ResponseItem::ContextCompaction { .. }
        | ResponseItem::Other => Ok(()),
    }
}

fn prepare_content_items(
    items: &mut [ContentItem],
    mode: PreparationMode,
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    for item in items {
        if let ContentItem::InputImage { image_url, detail } = item {
            prepare_image_url_if_needed(image_url, detail, mode)?;
        }
    }
    Ok(())
}

fn prepare_function_call_output_content_items(
    items: &mut [FunctionCallOutputContentItem],
    mode: PreparationMode,
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    for item in items {
        if let FunctionCallOutputContentItem::InputImage { image_url, detail } = item {
            prepare_image_url_if_needed(image_url, detail, mode)?;
        }
    }
    Ok(())
}

fn prepare_image_url_if_needed(
    image_url: &mut String,
    detail: &mut Option<ImageDetail>,
    mode: PreparationMode,
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    if matches!(mode, PreparationMode::UnpreparedImagesOnly)
        && detail.is_none()
        && is_data_url(image_url)
    {
        return Ok(());
    }
    prepare_image_url_for_responses_codex_strict_mode(image_url, detail)
}

fn prepare_image_url_for_responses_codex_strict_mode(
    image_url: &mut String,
    detail: &mut Option<ImageDetail>,
) -> Result<(), ResponsesCodexStrictImagePreparationError> {
    if !is_data_url(image_url) {
        return Err(ResponsesCodexStrictImagePreparationError::NonDataUrl {
            image_url_preview: image_url.chars().take(128).collect(),
        });
    }

    // Local-image and view_image producers may have already prepared their data URLs.
    // Keep this pass as the strict-mode history contract; the shared image cache
    // and preserve-within-bounds path avoid a second lossy encode for common formats.
    let mode = prompt_image_mode_for_responses_codex_strict_detail(*detail)?;
    let image = load_data_url_for_prompt(image_url, mode)?;
    *image_url = image.into_data_url();
    *detail = None;
    Ok(())
}

fn is_data_url(image_url: &str) -> bool {
    image_url
        .get(..DATA_URL_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(DATA_URL_PREFIX))
}

fn prompt_image_mode_for_responses_codex_strict_detail(
    detail: Option<ImageDetail>,
) -> Result<PromptImageMode, ResponsesCodexStrictImagePreparationError> {
    match detail {
        None | Some(ImageDetail::Auto | ImageDetail::Original) => {
            Ok(PromptImageMode::ResponsesLiteOriginal)
        }
        Some(ImageDetail::High) => Ok(PromptImageMode::ResizeToFit),
        Some(ImageDetail::Low) => {
            Err(ResponsesCodexStrictImagePreparationError::UnsupportedLowDetail)
        }
    }
}

#[cfg(test)]
#[path = "responses_strict_images_tests.rs"]
mod tests;
