use coverly_core::image::{
    probe_dimensions, process_image, Fit, ImageError, ImageLimits, OutputSpec,
};
use coverly_core::net::testing::png_bytes;
use coverly_core::profiles::{builtin_profiles, ImageFormat, Media};

fn spec(format: ImageFormat, fit: Fit) -> OutputSpec {
    OutputSpec {
        format,
        max_size: Some((250, 360)),
        fit,
        jpeg_quality: 90,
    }
}

#[test]
fn shrinks_into_the_box_keeping_aspect() {
    let out = process_image(
        &png_bytes(1000, 500),
        &spec(ImageFormat::Png, Fit::Contain),
        &ImageLimits::default(),
    )
    .unwrap();
    assert_eq!((out.width, out.height), (250, 125));
    assert_eq!(probe_dimensions(&out.bytes), Some((250, 125)));
    assert!(out.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn never_upscales() {
    let out = process_image(
        &png_bytes(100, 80),
        &spec(ImageFormat::Png, Fit::Contain),
        &ImageLimits::default(),
    )
    .unwrap();
    assert_eq!((out.width, out.height), (100, 80));
}

#[test]
fn pad_fills_the_whole_box() {
    let out = process_image(
        &png_bytes(1000, 500),
        &spec(ImageFormat::Png, Fit::Pad { rgba: [0, 0, 0, 0] }),
        &ImageLimits::default(),
    )
    .unwrap();
    assert_eq!(probe_dimensions(&out.bytes), Some((250, 360)));
}

#[test]
fn encodes_jpeg_when_the_slot_asks() {
    let out = process_image(
        &png_bytes(400, 400),
        &spec(ImageFormat::Jpg, Fit::Contain),
        &ImageLimits::default(),
    )
    .unwrap();
    assert!(out.bytes.starts_with(&[0xFF, 0xD8]));
    assert_eq!(probe_dimensions(&out.bytes), Some((250, 250)));
}

#[test]
fn rejects_non_images() {
    let err = process_image(
        b"<html><body>404 Not Found</body></html>",
        &spec(ImageFormat::Png, Fit::Contain),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(err, ImageError::NotAnImage);
}

#[test]
fn rejects_truncated_images() {
    let png = png_bytes(64, 64);
    let err = process_image(
        &png[..60],
        &spec(ImageFormat::Png, Fit::Contain),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert!(matches!(err, ImageError::Decode(_)), "{err:?}");
}

#[test]
fn rejects_oversized_dimensions() {
    let err = process_image(
        &png_bytes(9000, 1),
        &spec(ImageFormat::Png, Fit::Contain),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        ImageError::Dimensions {
            width: 9000,
            height: 1
        }
    );
}

#[test]
fn rejects_oversized_downloads() {
    let limits = ImageLimits {
        max_input_bytes: 10,
        ..ImageLimits::default()
    };
    assert!(matches!(
        process_image(
            &png_bytes(8, 8),
            &spec(ImageFormat::Png, Fit::Contain),
            &limits
        ),
        Err(ImageError::TooLarge { .. })
    ));
}

#[test]
fn output_spec_from_the_anbernic_slot() {
    let p = builtin_profiles()
        .into_iter()
        .find(|p| p.id == "anbernic-stock")
        .unwrap();
    let s = OutputSpec::for_slot(p.slot(Media::Cover).unwrap(), Fit::Contain, 90);
    assert_eq!((s.format, s.max_size), (ImageFormat::Png, Some((250, 360))));
}
