use axum::body::to_bytes;
use axum::http::{header, Request, StatusCode};
use image::{codecs::gif::GifEncoder, Delay, Frame, ImageBuffer, Rgba, RgbaImage};
use jxlify::cache::CacheManager;
use jxlify::config::JxlifyConfig;
use jxlify::handler::{build_router, AppState};
use std::fs::File;
use std::sync::Arc;
use tempfile::TempDir;
use tower::util::ServiceExt;

fn create_test_app(temp_dir: &TempDir) -> (axum::Router, JxlifyConfig) {
    let data_dir = temp_dir.path().join("data");
    let img_dir = data_dir.join("pics");
    let cache_dir = data_dir.join("cache");

    std::fs::create_dir_all(&img_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let mut config = JxlifyConfig::default();
    config.img_path = img_dir.to_string_lossy().to_string();
    config.cache_path = cache_dir.to_string_lossy().to_string();
    config.enable_extra_params = true;
    config.sync_convert_flags();

    let state = AppState {
        config: Arc::new(config.clone()),
        cache: CacheManager::new(),
        http_client: reqwest::Client::new(),
    };

    let router = build_router(state);
    (router, config)
}

fn generate_test_rgba_image(with_alpha: bool) -> RgbaImage {
    ImageBuffer::from_fn(200, 160, |x, y| {
        let r = ((x * 7 + y * 3) % 255) as u8;
        let g = ((x * 5 + y * 11) % 255) as u8;
        let b = ((x * 13 + y * 7) % 255) as u8;
        let a = if with_alpha && x < 50 && y < 50 { 0 } else { 255 };
        Rgba([r, g, b, a])
    })
}

fn create_test_image(img_dir: &std::path::Path, filename: &str, with_alpha: bool) {
    let target = img_dir.join(filename);
    let rgba = generate_test_rgba_image(with_alpha);
    let dynamic = image::DynamicImage::ImageRgba8(rgba);

    if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
        dynamic.to_rgb8().save(target).unwrap();
    } else if filename.ends_with(".avif") {
        jxlify::encoder::avif::encode_avif(&dynamic, &target, 80).unwrap();
    } else if filename.ends_with(".webp") {
        jxlify::encoder::webp::encode_webp(&dynamic, &target, 80).unwrap();
    } else if filename.ends_with(".jxl") {
        jxlify::encoder::jxl::encode_jxl(&dynamic, &target, 80).unwrap();
    } else {
        dynamic.save(target).unwrap();
    }
}

fn create_test_animated_gif(img_dir: &std::path::Path, filename: &str) {
    let target = img_dir.join(filename);
    let file = File::create(target).unwrap();
    let mut encoder = GifEncoder::new(file);

    let frame1: RgbaImage = ImageBuffer::from_fn(60, 60, |x, _y| {
        if x < 30 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 0, 0]) }
    });
    let frame2: RgbaImage = ImageBuffer::from_fn(60, 60, |_x, y| {
        if y < 30 { Rgba([0, 255, 0, 255]) } else { Rgba([0, 0, 0, 0]) }
    });

    encoder.encode_frame(Frame::from_parts(frame1, 0, 0, Delay::from_numer_denom_ms(100, 1))).unwrap();
    encoder.encode_frame(Frame::from_parts(frame2, 0, 0, Delay::from_numer_denom_ms(100, 1))).unwrap();
}

#[tokio::test]
async fn test_healthz_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let (app, _) = create_test_app(&temp_dir);

    let response = app
        .oneshot(Request::builder().uri("/healthz").body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["service"], "jxlify");
}

#[tokio::test]
async fn test_content_negotiation_jxl() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "photo.jpg", false);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/photo.jpg")
                .header(header::ACCEPT, "image/jxl,image/avif,image/webp,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/jxl");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "jxl");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "jpg");
    assert!(response.headers().contains_key(header::ETAG));
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept, User-Agent");
}

#[tokio::test]
async fn test_content_negotiation_avif() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "sample.png", true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sample.png")
                .header(header::ACCEPT, "image/avif,image/webp,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/avif");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "avif");
}

#[tokio::test]
async fn test_content_negotiation_webp() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "sample.png", true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sample.png")
                .header(header::ACCEPT, "image/webp,image/apng,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/webp");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "webp");
}

#[tokio::test]
async fn test_transparent_png_conversion() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "alpha.png", true);

    // Request WebP
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/alpha.png")
                .header(header::ACCEPT, "image/webp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/webp");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(!bytes.is_empty());
}

#[tokio::test]
async fn test_animated_gif_conversion_webp() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_animated_gif(std::path::Path::new(&config.img_path), "animated.gif");

    // Request WebP from animated GIF
    let response = app
        .oneshot(
            Request::builder()
                .uri("/animated.gif")
                .header(header::ACCEPT, "image/webp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/webp");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "webp");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "gif");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(!bytes.is_empty());
}

#[tokio::test]
async fn test_animated_gif_conversion_avif() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_animated_gif(std::path::Path::new(&config.img_path), "animated.gif");

    // Request AVIF from animated GIF
    let response = app
        .oneshot(
            Request::builder()
                .uri("/animated.gif")
                .header(header::ACCEPT, "image/avif,image/webp,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/avif");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "avif");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "gif");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(!bytes.is_empty());
}

#[tokio::test]
async fn test_animated_gif_conversion_jxl() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_animated_gif(std::path::Path::new(&config.img_path), "animated.gif");

    // Request JXL from animated GIF
    let response = app
        .oneshot(
            Request::builder()
                .uri("/animated.gif")
                .header(header::ACCEPT, "image/jxl,image/avif,image/webp,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/jxl");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "jxl");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "gif");
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(!bytes.is_empty());
}

#[test]
fn test_real_world_animated_gif_conversion() {
    let sample = std::path::Path::new("/home/liu/Downloads/f166430961aee11960e6672d3c1997c0ae7ae3f832fdb904359adfd1db6bf9be.gif");
    if !sample.exists() {
        return;
    }
    let temp_dir = TempDir::new().unwrap();
    let avif_out = temp_dir.path().join("out.avif");
    let jxl_out = std::path::PathBuf::from("/tmp/test_jxlify.jxl");
    let webp_out = temp_dir.path().join("out.webp");

    jxlify::encoder::animated::convert_animated_gif_to_avif(sample, &avif_out, 80).unwrap();
    assert!(avif_out.exists() && std::fs::metadata(&avif_out).unwrap().len() > 0);

    jxlify::encoder::animated::convert_animated_gif_to_jxl(sample, &jxl_out, 80).unwrap();
    assert!(jxl_out.exists() && std::fs::metadata(&jxl_out).unwrap().len() > 0);

    jxlify::encoder::animated::convert_animated_gif_to_webp(sample, &webp_out, 80).unwrap();
    assert!(webp_out.exists() && std::fs::metadata(&webp_out).unwrap().len() > 0);

    println!(
        "Original GIF: {} bytes | AVIF: {} bytes | JXL: {} bytes | WebP: {} bytes",
        std::fs::metadata(sample).unwrap().len(),
        std::fs::metadata(&avif_out).unwrap().len(),
        std::fs::metadata(&jxl_out).unwrap().len(),
        std::fs::metadata(&webp_out).unwrap().len(),
    );
}

#[tokio::test]
async fn test_on_demand_resizing_and_crop() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "large.png", false);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/large.png?width=40&height=30")
                .header(header::ACCEPT, "image/webp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!(decoded.width(), 40);
    assert_eq!(decoded.height(), 30);
}

#[tokio::test]
async fn test_metadata_full_endpoint() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "inspect.jpg", false);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/inspect.jpg?meta=full")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["width"], 200);
    assert_eq!(json["height"], 160);
    assert!(json["blurhash"].as_str().is_some());
}

#[tokio::test]
async fn test_etag_if_none_match() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "cacheable.jpg", false);

    // Initial request to get ETag
    let res1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/cacheable.jpg")
                .header(header::ACCEPT, "image/webp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res1.status(), StatusCode::OK);
    let etag = res1.headers().get(header::ETAG).unwrap().to_str().unwrap().to_string();

    // Subsequent request with If-None-Match
    let res2 = app
        .oneshot(
            Request::builder()
                .uri("/cacheable.jpg")
                .header(header::IF_NONE_MATCH, etag)
                .header(header::ACCEPT, "image/webp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res2.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn test_skip_conversion_when_raw_matches_target() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "native.webp", false);

    // Request WebP when original is already WebP
    let response = app
        .oneshot(
            Request::builder()
                .uri("/native.webp")
                .header(header::ACCEPT, "image/webp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/webp");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "webp");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "webp");
}

#[tokio::test]
async fn test_folder_sharding_structure() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "sharded_test.png", false);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sharded_test.png")
                .header(header::ACCEPT, "image/webp")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Inspect image cache directory for 2-level sharded directory structure
    let images_cache = config.images_cache_path().join("local");
    let mut found_sharded_file = false;
    for entry in walkdir::WalkDir::new(&images_cache).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("webp") {
            let relative = entry.path().strip_prefix(&images_cache).unwrap();
            // Should have at least 3 components: prefix1 / prefix2 / id.webp
            let components: Vec<_> = relative.components().collect();
            assert_eq!(components.len(), 3, "Expected 2-level sharded path: {:?}", relative);
            found_sharded_file = true;
        }
    }
    assert!(found_sharded_file, "Sharded cached webp file should exist");
}

#[tokio::test]
async fn test_disallowed_extension_returns_400() {
    let temp_dir = TempDir::new().unwrap();
    let (app, _) = create_test_app(&temp_dir);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/script.sh")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_path_traversal_returns_404() {
    let temp_dir = TempDir::new().unwrap();
    let (app, _) = create_test_app(&temp_dir);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/../../etc/passwd")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn test_toml_config_loading() {
    let temp_dir = TempDir::new().unwrap();
    let toml_path = temp_dir.path().join("config.toml");
    std::fs::write(
        &toml_path,
        r#"
host = "0.0.0.0"
port = 8888
quality = 92
img_path = "./my_pics"
cache_path = "./my_cache"
convert_types = ["jxl", "webp"]
enable_extra_params = true
"#,
    )
    .unwrap();

    let config = JxlifyConfig::load_from_file_or_default(&toml_path.to_string_lossy());
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, "8888");
    assert_eq!(config.quality, 92);
    assert_eq!(config.img_path, "./my_pics");
    assert_eq!(config.cache_path, "./my_cache");
    assert_eq!(config.convert_types, vec!["jxl", "webp"]);
    assert!(config.enable_jxl);
    assert!(!config.enable_avif);
    assert!(config.enable_webp);
    assert!(config.enable_extra_params);
}

#[tokio::test]
async fn test_avif_raw_converted_to_jxl_when_browser_supports_jxl() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "source.avif", false);

    // Browser supports JXL
    let response = app
        .oneshot(
            Request::builder()
                .uri("/source.avif")
                .header(header::ACCEPT, "image/jxl,image/avif,image/webp,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/jxl");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "jxl");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "avif");
}

#[tokio::test]
async fn test_avif_raw_served_directly_when_browser_only_supports_avif() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "source.avif", false);

    // Browser does NOT support JXL, but supports AVIF
    let response = app
        .oneshot(
            Request::builder()
                .uri("/source.avif")
                .header(header::ACCEPT, "image/avif,image/webp,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/avif");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "avif");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "avif");

    // Verify no image cache file was created (served raw directly)
    let images_cache = config.images_cache_path();
    let mut cached_files = 0;
    for entry in walkdir::WalkDir::new(&images_cache).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            cached_files += 1;
        }
    }
    assert_eq!(cached_files, 0, "No cache file should be generated when serving raw directly");
}

#[tokio::test]
async fn test_format_query_param_override() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "sample.avif", false);

    // Browser sends Accept for avif/webp, but URL specifies ?format=jxl
    let response = app
        .oneshot(
            Request::builder()
                .uri("/sample.avif?format=jxl")
                .header(header::ACCEPT, "image/avif,image/webp,*/*;q=0.8")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/jxl");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "jxl");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "avif");
}

#[tokio::test]
async fn test_fuzzy_stem_resolution() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    // Only sample.avif exists on disk
    create_test_image(std::path::Path::new(&config.img_path), "sample.avif", false);

    // Client requests sample.jxl directly
    let response = app
        .oneshot(
            Request::builder()
                .uri("/sample.jxl")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/jxl");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "jxl");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "avif");
}

#[tokio::test]
async fn test_ua_heuristic_thorium_serves_jxl() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    create_test_image(std::path::Path::new(&config.img_path), "sample.avif", false);

    // Thorium browser sends document navigation Accept header (no image/jxl)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/sample.avif")
                .header(header::ACCEPT, "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                .header(header::USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 Thorium/122.0.6261.128")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/jxl");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "jxl");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "avif");
}

#[tokio::test]
async fn test_jxl_raw_converted_to_avif_when_browser_only_supports_avif() {
    let temp_dir = TempDir::new().unwrap();
    let (app, config) = create_test_app(&temp_dir);
    // Raw image on disk is JXL
    create_test_image(std::path::Path::new(&config.img_path), "sample.jxl", false);

    // Browser does NOT support JXL, only AVIF
    let response = app
        .oneshot(
            Request::builder()
                .uri("/sample.jxl")
                .header(header::ACCEPT, "image/avif,image/webp,*/*;q=0.8")
                .header(header::USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/avif");
    assert_eq!(response.headers().get("X-Served-Format").unwrap(), "avif");
    assert_eq!(response.headers().get("X-Original-Format").unwrap(), "jxl");
}


