<div align="center">

# ⚡ JXLify

**A Next-Generation Image Proxy & Origin Server in Rust**  
*Serving JPEG XL (`.jxl`), AVIF (`.avif`), WebP (`.webp`), and Legacy Formats on the Fly with 100% In-Process Rust Codecs, Smart Content Negotiation, and Cache Acceleration.*

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![Crates.io](https://img.shields.io/crates/v/jxlify.svg)](https://crates.io/crates/jxlify)
[![Documentation](https://docs.rs/jxlify/badge.svg)](https://docs.rs/jxlify)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![JPEG XL](https://img.shields.io/badge/JPEG%20XL-Supported-green.svg)](https://jpegxl.info)
[![AVIF](https://img.shields.io/badge/AVIF-Supported-blue.svg)](https://aomedia.org/av1-features/avif/)
[![WebP](https://img.shields.io/badge/WebP-Supported-brightgreen.svg)](https://developers.google.com/speed/webp)

</div>

---

## 🌟 Overview

**JXLify** is a high-performance image proxy and origin web server written in **Rust**, designed as a modern, memory-safe, asynchronous evolution of [`webp_server_go`](https://github.com/webp-sh/webp_server_go).

JXLify sits as a **middle-man in the loop** between your users/CDN and your raw image assets. When a client requests an image (e.g. `https://example.com/photos/landscape.jpg`), JXLify dynamically negotiates client support using HTTP `Accept` headers and `User-Agent` heuristics, converts the image on the fly to the optimal format (**JPEG XL**, **AVIF**, or **WebP**), caches the generated asset and its metadata on disk, and serves it with instant response times and minimal CPU usage.

---

## 🧭 Format Fallback Matrix & Negotiation Strategy

### The Fallback Hierarchy
JXLify negotiates image formats using a 4-tier hierarchy:

$$\mathbf{JPEG\ XL\ (jxl)} \;\longrightarrow\; \mathbf{AVIF\ (avif)} \;\longrightarrow\; \mathbf{WebP\ (webp)} \;\longrightarrow\; \mathbf{Original\ (JPEG/PNG/GIF/BMP)}$$

### Targeted Single-Format Encoding (CPU-Efficient)
Instead of converting all formats simultaneously on every request (which wastes CPU and increases latency), JXLify determines the single **highest-priority format** the client supports and converts/serves **only that format**:

1. **Client supports JPEG XL (`image/jxl`):** Serves cached JXL or encodes to JXL.
2. **Client supports AVIF (`image/avif`):** Serves cached AVIF or encodes to AVIF.
3. **Client supports WebP (`image/webp`):** Serves cached WebP or encodes to WebP.
4. **Client supports none of the above:** Serves the original raw image (or resized raw image).

### ⚡ Redundant Conversion Skip
If the source raw image is already in the negotiated format (e.g. source is already `photo.webp` and client accepts WebP) and no resizing/cropping is requested, JXLify **skips conversion entirely** and serves the file directly with 0ms CPU overhead.

---

## 🗄️ Unified Storage Layout & 2-Level Hash Sharding

All data is cleanly organized under the `data/` directory:
- `data/pics`: Original source images.
- `data/cache`: Unified disk cache directory (**safe to delete at any time**):
  - `data/cache/images/`: Converted `.jxl`, `.avif`, `.webp` and resized variants (2-level sharded).
  - `data/cache/metadata/`: JSON metadata & BlurHash strings (2-level sharded).
  - `data/cache/remote/`: Downloaded upstream images in reverse proxy mode (2-level sharded).

---

## 🗂️ Understanding & Configuring Storage Paths

In `config.toml`, only two paths are needed:

```toml
img_path = "./data/pics"     # Source images (local path or remote URL)
cache_path = "./data/cache"  # Unified cache (safe to delete anytime)
```

| Config Key | Purpose | Typical Value | Description |
| :--- | :--- | :--- | :--- |
| **`img_path`** | **Original Image Source** | `"./data/pics"` or `"https://cdn.example.com"` | Directory (or upstream URL) containing raw source images. |
| **`cache_path`** | **Unified Disk Cache** | `"./data/cache"` | Cache directory for all generated data (`images/`, `metadata/`, `remote/`). Safe to delete at any time (`rm -rf ./data/cache`). |

---

### How to Setup Storage

#### Mode 1: Local Origin Mode (Self-Hosted Images)
If your original image files are hosted on the local server or mounted volume:
1. Put your source images into `./data/pics/` (or any path like `/var/www/uploads/`).
2. Set `img_path = "./data/pics"`.
3. Set `cache_path = "./data/cache"`.
4. Requests to `http://localhost:3333/photos/cat.jpg` will resolve to `./data/pics/photos/cat.jpg` and cache to `./data/cache/images/...`.

#### Mode 2: Remote Reverse-Proxy Mode (CDN / S3 / Cloud Storage)
If you want JXLify to sit in front of an existing remote image server:
1. Set `img_path = "https://origin.example.com/assets"`.
2. Requests to `http://localhost:3333/photos/cat.jpg` will fetch `https://origin.example.com/assets/photos/cat.jpg`.
3. JXLify caches the raw file in `data/cache/remote/`, converts it to the client's optimal format, caches the result in `data/cache/images/`, and serves it instantly.

---

### Production Directory Sharding
In high-volume production environments with millions of images, flat directories cause severe filesystem inode lock contention and slow directory lookups. JXLify automatically implements **2-level hash sharding**:

```
data/cache/images/local/
├── 8f/
│   └── 32/
│       ├── 8f32d4639f7f415f.jxl
│       ├── 8f32d4639f7f415f.avif
│       └── 8f32d4639f7f415f.webp
├── 01/
│   └── ee/
│       └── 01eef898821f07c6.webp
```

This distributes millions of files evenly across $256 \times 256 = 65,536$ subdirectories, maintaining $O(1)$ fast file lookups on all filesystems.

---

## 🦀 100% In-Process Codecs & Transparency Preservation

**No external CLI tools (FFmpeg / ImageMagick) are required.** JXLify processes all formats entirely in-process:

### 1. Static Image Processing & Alpha Channels
- **JPEG XL:** In-process via `jpegxl-rs` (libjxl) and `jxl-encoder`. Preserves full alpha transparency (`RGBA8`/`RGBA16`).
- **AVIF:** Pure Rust via `ravif` / `rav1e` (the engine behind `cavif-rs`). Preserves full alpha transparency.
- **WebP:** In-process via `webp` and `image` / `image-webp`. Preserves alpha transparency.
- **JPEG / PNG / BMP / GIF:** Pure Rust decoding and encoding via `image`.

### 2. Animated GIF & WebP Processing
- **Decoding:** Multi-frame GIFs are parsed in-process via `image::codecs::gif::GifDecoder`, extracting all frames, transparent palettes, and frame delays.
- **Encoding:** Converted directly to animated WebP in-process using `webp-animation`, preserving frame rates, loop counts, and alpha transparency.
- **Pass-through:** Original animated GIF/WebP files are served untouched for legacy clients.

---

## ⚡ Key Features

- 🚀 **Asynchronous & Multi-Threaded:** Built on **Axum** and **Tokio** with asynchronous I/O and Rayon thread pool for fast encoding.
- 🎯 **Intelligent Content Negotiation:** Evaluates `Accept` header MIME types and `User-Agent` heuristics (e.g. Safari 17+, iOS 17+, Firefox >= 93, Chrome).
- 🔄 **On-The-Fly Conversion:** Automatic conversion of `jpg`, `png`, `gif`, `bmp`, `svg`, `heic`, `nef` into `jxl`, `avif`, or `webp`.
- 🗄️ **Persistent Disk Caching with Hash Sharding:** Caches optimized images and metadata in `CACHE_PATH` with 2-level directory sharding.
- 🔒 **In-Flight Deduplication Lock:** Uses async lock registry (`DashMap`) to eliminate duplicate encoding when multiple concurrent requests hit the same uncached image.
- 🌐 **Local Origin & Remote Proxy Modes:** Can serve from local directory (`IMG_PATH: "./data/pics"`) or act as a reverse proxy for remote CDNs (`IMG_PATH: "https://origin.example.com"`).
- 📐 **On-Demand Resizing & Smart Cropping:** Supports query parameters `?width=300&height=200`, `?max_width=800&max_height=600` with multiple crop algorithms.
- 🔍 **Image Metadata Endpoint:** Append `?meta=full` to retrieve image dimensions, color profile, size, and [BlurHash](https://blurha.sh/) string.
- 🧹 **Automatic Cache Cleaner:** Periodically enforces `MAX_CACHE_SIZE` using LRU / modification time pruning.
- 📦 **Prefetching:** Multi-threaded batch scanner (`--prefetch` / `--prefetch-foreground`) to pre-warm the cache before production deployment.
- 📊 **HTTP Caching & Metrics:** Sends `Vary: Accept, User-Agent`, weak `ETag`, `Cache-Control`, and `X-Compression-Rate`.
- 🩺 **Health Check:** Built-in `/healthz` endpoint for Kubernetes and load balancer monitoring.

---

## 🚀 Quick Start

### Installation

#### Install via Cargo (Binary)

You can install the `jxlify` server binary directly from [crates.io](https://crates.io/crates/jxlify):

```bash
cargo install jxlify
```

Once installed, verify the installation by running:

```bash
jxlify --help
```

#### Use as a Library Dependency

Add `jxlify` to your project's `Cargo.toml`:

```toml
[dependencies]
jxlify = "0.1"
```

Or add it via the command line:

```bash
cargo add jxlify
```

#### Build from Source

```bash
git clone https://github.com/fokx/jxlify.git
cd jxlify
cargo build --release
```

The compiled binary will be located at `target/release/jxlify`.

---

## ⚙️ Configuration Reference

Generate a default `config.toml`:

```bash
./target/release/jxlify --dump-config > config.toml
```

### Full Configuration Table

| Option | Type | Default | Description                                                                                                                                                | Environment Override |
| :--- | :--- | :--- |:-----------------------------------------------------------------------------------------------------------------------------------------------------------| :--- |
| **`host`** | `String` | `"0.0.0.0"` | IP address to bind (use `127.0.0.1` for localhost only, `0.0.0.0` for all interfaces).                                                                     | `JXLIFY_HOST` |
| **`port`** | `String`/`Int`| `"3333"` | TCP port for the HTTP server.                                                                                                                              | `JXLIFY_PORT` |
| **`quality`** | `Integer` | `80` | Compression quality (1–100). `80` offers visually lossless quality; `100` triggers true lossless encoding for JXL and WebP.                                | `JXLIFY_QUALITY` |
| **`allowed_types`** | `Array` | `["jpg", "png", ...]` | Whitelist of allowed image extensions. Requests for other extensions return `400 Bad Request`. Use `["*"]` to allow all.                                   | `JXLIFY_ALLOWED_TYPES` |
| **`convert_types`** | `Array` | `["jxl", "avif", "webp"]` | Enabled modern output formats. Order does not matter (fallback priority is always `JXL -> AVIF -> WebP`). Remove a format (e.g. `["webp"]`) to disable it. | `JXLIFY_CONVERT_TYPES` |
| **`strip_metadata`** | `Boolean` | `true` | When `true`, strips EXIF, GPS coordinates, and camera profiles from output files to minimize size and protect privacy.                                     | `JXLIFY_STRIP_METADATA` |
| **`img_path`** | `String` | `"./data/pics"` | Root directory or remote HTTP/HTTPS upstream URL for source images.                                                                                        | `JXLIFY_IMG_PATH` |
| **`cache_path`** | `String` | `"./data/cache"` | Unified cache directory for all generated data (`images/`, `metadata/`, `remote/`).                                                                        | `JXLIFY_CACHE_PATH` |
| **`enable_extra_params`**| `Boolean` | `false` | When `true`, enables on-demand dynamic resizing via query parameters (`?width=`, `?height=`, `?max_width=`, `?max_height=`).                               | `JXLIFY_ENABLE_EXTRA_PARAMS` |
| **`crop_interesting`** | `String` | `"InterestingAttention"` | Focal point algorithm for aspect-ratio crops. Options: `InterestingAttention`, `InterestingEntropy`, `InterestingCentre`, `InterestingNone`.               | `JXLIFY_EXTRA_PARAMS_CROP_INTERESTING` |
| **`cache_ttl`** | `Integer` | `2592000` | Browser cache TTL in seconds (default `2592000` = 30 days). Sent in `Cache-Control: max-age=...`.                                                          | `JXLIFY_CACHE_TTL` |
| **`max_cache_size`** | `Integer` | `0` | Maximum disk cache size in Megabytes (e.g. `10240` for 10 GB). `0` = unlimited. Background cleaner automatically removes oldest LRU files when exceeded.   | `JXLIFY_MAX_CACHE_SIZE` |
| **`read_buffer_size`** | `Integer` | `4096` | Internal I/O read buffer size in bytes for streaming files.                                                                                                | `JXLIFY_READ_BUFFER_SIZE` |
| **`concurrency`** | `Integer` | `262144` | Maximum concurrent request worker pool size.                                                                                                               | `JXLIFY_CONCURRENCY` |
| **`disable_keepalive`** | `Boolean` | `false` | Set to `true` to close TCP connection after each request (`Connection: close`).                                                                            | `JXLIFY_DISABLE_KEEPALIVE` |

---

### Detailed Setting Explanations

#### 1. Image Quality (`quality = 80`)
- Controls lossy compression density across all encoders (1–100).
- **80** is the recommended default, providing ~70–85% file size reduction with zero visible artifacting.
- **100** activates lossless compression mode in libjxl (JPEG XL) and libwebp (WebP).

#### 2. Format Whitelist & Target Formats
- **`allowed_types`**: Security control preventing arbitrary file serving. Only extensions in this list will be processed (e.g. `jpg`, `jpeg`, `png`, `gif`, `bmp`, `svg`, `heic`, `nef`, `webp`, `avif`, `jxl`).
- **`convert_types`**: Allows selectively disabling newer formats if desired. For example, setting `convert_types = ["avif", "webp"]` disables JXL conversion even if client supports it.

#### 3. Dynamic Resizing & Smart Cropping (`enable_extra_params = true`)
When enabled, JXLify dynamically resizes images based on URL query parameters:
- `GET /photo.jpg?width=400&height=300`: Resizes and crops to exact 400x300 dimensions using the configured smart crop algorithm (`crop_interesting`).
- `GET /photo.jpg?max_width=800&max_height=600`: Proportionally fits image within bounding box while preserving original aspect ratio.

#### 4. Cache Cleanup & Quotas (`max_cache_size`)
- If `max_cache_size = 10240` (10 GB), a lightweight background cleaner runs every 60 seconds.
- If total cache size exceeds the limit, the cleaner automatically purges the oldest, least-recently-modified files (**LRU policy**) until cache size is within limits.
- Also cleans stale `.tmp.*` temporary files from interrupted writes.

---

## 📖 CLI Usage

```bash
# Start server with default ./config.toml
./target/release/jxlify

# Specify custom config file
./target/release/jxlify --config /etc/jxlify/config.toml

# Prefetch all images in background while server runs
./target/release/jxlify --prefetch --jobs 8

# Prefetch all images in foreground and exit
./target/release/jxlify --prefetch-foreground --jobs 8

# Print version
./target/release/jxlify -V
```

---

## 🐧 Running as a Daemon (Systemd Service)

A production-ready systemd unit file is provided at [`jxlify.service`](jxlify.service).

### 1. Standard System Installation
```bash
# 1. Install binary to /usr/bin/
sudo install -Dm755 target/release/jxlify /usr/bin/jxlify

# 2. Install default configuration to /etc/jxlify/
sudo install -Dm644 config.toml /etc/jxlify/config.toml

# 3. Install systemd service unit
sudo install -Dm644 jxlify.service /etc/systemd/system/jxlify.service

# 4. Reload systemd daemon
sudo systemctl daemon-reload
```

### 2. Enable & Start
```bash
# Start and enable JXLify on boot
sudo systemctl enable --now jxlify

# Check service status
sudo systemctl status jxlify
```

### 3. View Logs
```bash
# Stream live logs
journalctl -u jxlify -f
```

---

## 📡 HTTP API & Headers

### Requesting Images
```bash
# Request image (Safari 17+ will receive image/jxl, Chrome will receive image/avif, older browsers get image/webp or JPEG)
curl -i -H "Accept: image/jxl,image/avif,image/webp,image/*,*/*;q=0.8" http://localhost:3333/sample.jpg

# Request image with on-the-fly resizing
curl -i http://localhost:3333/sample.jpg?width=400&height=300

# Inspect metadata & BlurHash
curl -i http://localhost:3333/sample.jpg?meta=full
```

### Response Headers Example
```http
HTTP/1.1 200 OK
Content-Type: image/jxl
Content-Length: 42150
Vary: Accept, User-Agent
ETag: W/"a9b8c7d6e5"
Cache-Control: public, max-age=31536000, immutable
X-Original-Format: jpeg
X-Served-Format: jxl
X-Compression-Rate: 0.38
Server: JXLify
```

---

## 📄 License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
