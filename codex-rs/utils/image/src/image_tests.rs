use std::io::Cursor;

use super::*;
use image::GenericImageView;
use image::ImageBuffer;
use image::Rgba;

fn image_bytes(image: &ImageBuffer<Rgba<u8>, Vec<u8>>, format: ImageFormat) -> Vec<u8> {
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut encoded, format)
        .expect("encode image to bytes");
    encoded.into_inner()
}

#[tokio::test(flavor = "multi_thread")]
async fn returns_original_image_when_within_bounds() {
    for (format, mime) in [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::WebP, "image/webp"),
    ] {
        let image = ImageBuffer::from_pixel(64, 32, Rgba([10u8, 20, 30, 255]));
        let original_bytes = image_bytes(&image, format);

        let encoded = load_for_prompt_bytes(
            Path::new("in-memory-image"),
            original_bytes.clone(),
            PromptImageMode::ResizeToFit,
        )
        .expect("process image");

        assert_eq!(encoded.width, 64);
        assert_eq!(encoded.height, 32);
        assert_eq!(encoded.mime, mime);
        assert_eq!(encoded.bytes, original_bytes);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn downscales_large_image() {
    for (format, mime) in [
        (ImageFormat::Png, "image/png"),
        (ImageFormat::WebP, "image/webp"),
    ] {
        let image = ImageBuffer::from_pixel(4096, 2048, Rgba([200u8, 10, 10, 255]));
        let original_bytes = image_bytes(&image, format);

        let processed = load_for_prompt_bytes(
            Path::new("in-memory-image"),
            original_bytes,
            PromptImageMode::ResizeToFit,
        )
        .expect("process image");

        assert!(processed.width <= MAX_DIMENSION);
        assert!(processed.height <= MAX_DIMENSION);
        assert_eq!(processed.mime, mime);

        let detected_format =
            image::guess_format(&processed.bytes).expect("detect resized output format");
        assert_eq!(detected_format, format);

        let loaded =
            image::load_from_memory(&processed.bytes).expect("read resized bytes back into image");
        assert_eq!(loaded.dimensions(), (processed.width, processed.height));
        assert!(
            prompt_image_patch_count(processed.width, processed.height) <= HIGH_DETAIL_MAX_PATCHES
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn high_detail_downscales_to_patch_budget() {
    let image = ImageBuffer::from_pixel(4096, 4096, Rgba([200u8, 10, 10, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Jpeg);

    let processed = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        original_bytes,
        PromptImageMode::ResizeToFit,
    )
    .expect("process image");

    assert!(processed.width <= HIGH_DETAIL_MAX_DIMENSION);
    assert!(processed.height <= HIGH_DETAIL_MAX_DIMENSION);
    assert!(prompt_image_patch_count(processed.width, processed.height) <= HIGH_DETAIL_MAX_PATCHES);
}

#[tokio::test(flavor = "multi_thread")]
async fn high_detail_preserves_images_within_gpt_5_5_patch_budget() {
    let image = ImageBuffer::from_pixel(2048, 1024, Rgba([10u8, 20, 30, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);

    let processed = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        original_bytes.clone(),
        PromptImageMode::ResizeToFit,
    )
    .expect("process image");

    assert_eq!(processed.width, 2048);
    assert_eq!(processed.height, 1024);
    assert_eq!(
        prompt_image_patch_count(processed.width, processed.height),
        2048
    );
    assert_eq!(processed.bytes, original_bytes);
}

#[tokio::test(flavor = "multi_thread")]
async fn load_data_url_for_prompt_downscales_large_image() {
    let image = ImageBuffer::from_pixel(4096, 2048, Rgba([200u8, 10, 10, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);
    let image_url = EncodedImage {
        bytes: original_bytes,
        mime: "image/png".to_string(),
        width: 4096,
        height: 2048,
    }
    .into_data_url();

    let processed = load_data_url_for_prompt(&image_url, PromptImageMode::ResizeToFit)
        .expect("process data URL image");

    assert!(processed.width <= MAX_DIMENSION);
    assert!(processed.height <= MAX_DIMENSION);
    assert_eq!(processed.mime, "image/png");
}

#[tokio::test(flavor = "multi_thread")]
async fn load_data_url_for_prompt_preserves_original_large_image() {
    let image = ImageBuffer::from_pixel(4096, 2048, Rgba([10u8, 20, 30, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);
    let image_url = EncodedImage {
        bytes: original_bytes.clone(),
        mime: "image/png".to_string(),
        width: 4096,
        height: 2048,
    }
    .into_data_url();

    let processed = load_data_url_for_prompt(&image_url, PromptImageMode::Original)
        .expect("process data URL image");

    assert_eq!(processed.width, 4096);
    assert_eq!(processed.height, 2048);
    assert_eq!(processed.bytes, original_bytes);
}

#[tokio::test(flavor = "multi_thread")]
async fn load_data_url_for_prompt_accepts_case_insensitive_markers() {
    let image = ImageBuffer::from_pixel(64, 32, Rgba([10u8, 20, 30, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);
    let image_url = EncodedImage {
        bytes: original_bytes.clone(),
        mime: "image/png".to_string(),
        width: 64,
        height: 32,
    }
    .into_data_url()
    .replacen("data:", "DATA:", 1)
    .replacen(";base64,", ";BASE64,", 1);

    let processed = load_data_url_for_prompt(&image_url, PromptImageMode::Original)
        .expect("process data URL image");

    assert_eq!(processed.width, 64);
    assert_eq!(processed.height, 32);
    assert_eq!(processed.bytes, original_bytes);
}

#[tokio::test(flavor = "multi_thread")]
async fn second_pass_preserves_prepared_jpeg_bytes_when_within_bounds() {
    let image = ImageBuffer::from_pixel(4096, 2048, Rgba([200u8, 10, 10, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Jpeg);

    let first = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        original_bytes,
        PromptImageMode::ResizeToFit,
    )
    .expect("process image");

    assert!(first.width <= MAX_DIMENSION);
    assert!(first.height <= MAX_DIMENSION);
    assert_eq!(first.mime, "image/jpeg");

    let prepared_image_url = first.clone().into_data_url();
    let second = load_data_url_for_prompt(&prepared_image_url, PromptImageMode::ResizeToFit)
        .expect("process prepared data URL image");

    assert_eq!(second.width, first.width);
    assert_eq!(second.height, first.height);
    assert_eq!(second.mime, first.mime);
    assert_eq!(second.bytes, first.bytes);
}

#[test]
fn image_dimensions_from_base64_payload_reads_image_header() {
    let image = ImageBuffer::from_pixel(320, 240, Rgba([10u8, 20, 30, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);
    let payload = BASE64_STANDARD.encode(original_bytes);

    let dimensions = image_dimensions_from_base64_payload(&payload)
        .expect("read dimensions from base64 image payload");

    assert_eq!(dimensions, (320, 240));
}

#[tokio::test(flavor = "multi_thread")]
async fn downscales_tall_image_to_fit_square_bounds() {
    let image = ImageBuffer::from_pixel(1024, 4096, Rgba([200u8, 10, 10, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);

    let processed = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        original_bytes,
        PromptImageMode::ResizeToFit,
    )
    .expect("process image");

    assert_eq!(processed.width, 512);
    assert_eq!(processed.height, MAX_DIMENSION);
    assert_eq!(processed.mime, "image/png");
}

#[tokio::test(flavor = "multi_thread")]
async fn preserves_large_image_in_original_mode() {
    let image = ImageBuffer::from_pixel(6401, 100, Rgba([180u8, 30, 30, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);

    let processed = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        original_bytes.clone(),
        PromptImageMode::Original,
    )
    .expect("process image");

    assert_eq!(processed.width, 6401);
    assert_eq!(processed.height, 100);
    assert_eq!(processed.mime, "image/png");
    assert_eq!(processed.bytes, original_bytes);
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_lite_original_downscales_to_dimension_budget() {
    let image = ImageBuffer::from_pixel(6401, 100, Rgba([180u8, 30, 30, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);

    let processed = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        original_bytes,
        PromptImageMode::ResponsesLiteOriginal,
    )
    .expect("process image");

    assert!(processed.width <= ORIGINAL_DETAIL_MAX_DIMENSION);
    assert!(processed.height <= ORIGINAL_DETAIL_MAX_DIMENSION);
    assert!(
        prompt_image_patch_count(processed.width, processed.height) <= ORIGINAL_DETAIL_MAX_PATCHES
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_lite_original_downscales_to_patch_budget() {
    let image = ImageBuffer::from_pixel(3201, 3201, Rgba([180u8, 30, 30, 255]));
    let original_bytes = image_bytes(&image, ImageFormat::Png);

    let processed = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        original_bytes,
        PromptImageMode::ResponsesLiteOriginal,
    )
    .expect("process image");

    assert!(processed.width < 3201);
    assert!(processed.height < 3201);
    assert!(
        prompt_image_patch_count(processed.width, processed.height) <= ORIGINAL_DETAIL_MAX_PATCHES
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fails_cleanly_for_invalid_images() {
    let err = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        b"not an image".to_vec(),
        PromptImageMode::ResizeToFit,
    )
    .expect_err("invalid image should fail");
    assert!(matches!(
        err,
        ImageProcessingError::Decode { .. } | ImageProcessingError::UnsupportedImageFormat { .. }
    ));
}

#[test]
fn allows_prompt_image_inputs_at_size_limit() {
    ensure_prompt_image_input_size("base64 payload", MAX_PROMPT_IMAGE_INPUT_BYTES)
        .expect("base64 payload at the limit should be accepted");
    ensure_prompt_image_input_size("decoded input", MAX_PROMPT_IMAGE_INPUT_BYTES)
        .expect("decoded input at the limit should be accepted");
}

#[test]
fn rejects_base64_prompt_image_inputs_over_size_limit() {
    let size = MAX_PROMPT_IMAGE_INPUT_BYTES + 1;
    let err = ensure_prompt_image_input_size("base64 payload", size)
        .expect_err("oversized base64 payload should fail");

    assert!(matches!(
        err,
        ImageProcessingError::ImageTooLarge {
            representation: "base64 payload",
            size: got_size,
            max: MAX_PROMPT_IMAGE_INPUT_BYTES,
        } if got_size == size
    ));
}

#[test]
fn rejects_decoded_prompt_image_inputs_over_size_limit() {
    let size = MAX_PROMPT_IMAGE_INPUT_BYTES + 1;
    let err = ensure_prompt_image_input_size("decoded input", size)
        .expect_err("oversized decoded input should fail");

    assert!(matches!(
        err,
        ImageProcessingError::ImageTooLarge {
            representation: "decoded input",
            size: got_size,
            max: MAX_PROMPT_IMAGE_INPUT_BYTES,
        } if got_size == size
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn reprocesses_updated_file_contents() {
    {
        IMAGE_CACHE.clear();
    }

    let first_image = ImageBuffer::from_pixel(32, 16, Rgba([20u8, 120, 220, 255]));
    let first_bytes = image_bytes(&first_image, ImageFormat::Png);

    let first = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        first_bytes,
        PromptImageMode::ResizeToFit,
    )
    .expect("process first image");

    let second_image = ImageBuffer::from_pixel(96, 48, Rgba([50u8, 60, 70, 255]));
    let second_bytes = image_bytes(&second_image, ImageFormat::Png);

    let second = load_for_prompt_bytes(
        Path::new("in-memory-image"),
        second_bytes,
        PromptImageMode::ResizeToFit,
    )
    .expect("process updated image");

    assert_eq!(first.width, 32);
    assert_eq!(first.height, 16);
    assert_eq!(second.width, 96);
    assert_eq!(second.height, 48);
    assert_ne!(second.bytes, first.bytes);
}
