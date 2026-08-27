use clap::Parser;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

pub const DEFAULT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BANNER: &str = r#"
 ╦═╗┬ ┬┌─┐┌┬┐┬┌─┐┬ ┬   ╦═╗┬ ┬┌─┐┌┬┐
 ╠╦╝│ │└─┐ │ │├┤ └┬┘───╠╦╝│ │└─┐ │ 
 ╩╚═└─┘└─┘ ┴ ┴└   ┴    ╩╚═└─┘└─┘ ┴ 
 JXLify - Next-Gen Rust Image Proxy & Origin Server
"#;

pub const SAMPLE_TOML_CONFIG: &str = include_str!("../config.toml");

#[derive(Parser, Debug, Clone)]
#[command(name = "jxlify", author, version, about = "Next-Gen Image Proxy & Origin Server in Rust")]
pub struct CliArgs {
    #[arg(short, long, default_value = "config.toml", help = "Path to config.toml or config.json")]
    pub config: String,

    #[arg(long, default_value_t = false, help = "Prefetch images in background on launch")]
    pub prefetch: bool,

    #[arg(long, default_value_t = false, help = "Prefetch images in foreground and exit")]
    pub prefetch_foreground: bool,

    #[arg(short, long, default_value_t = num_cpus_helper(), help = "Prefetch worker threads")]
    pub jobs: usize,

    #[arg(short = 'l', long, default_value_t = 3, help = "Verbosity log level (0: silent, 1: error, 2: warn, 3: info, 4: debug)")]
    pub verbosity: u8,

    #[arg(long, default_value_t = false, help = "Print sample config.toml and exit")]
    pub dump_config: bool,
}

fn num_cpus_helper() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JxlifyConfig {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port", deserialize_with = "deserialize_port")]
    pub port: String,

    #[serde(default = "default_quality", deserialize_with = "deserialize_quality")]
    pub quality: u8,

    #[serde(default = "default_img_path")]
    pub img_path: String,

    #[serde(default = "default_cache_path")]
    pub cache_path: String,

    #[serde(default)]
    pub img_map: HashMap<String, String>,

    #[serde(default = "default_allowed_types")]
    pub allowed_types: Vec<String>,

    #[serde(default = "default_convert_types")]
    pub convert_types: Vec<String>,

    #[serde(default = "default_true")]
    pub strip_metadata: bool,

    #[serde(default = "default_false")]
    pub enable_extra_params: bool,

    #[serde(default = "default_crop")]
    pub crop_interesting: String,

    #[serde(default = "default_buffer_size")]
    pub read_buffer_size: usize,

    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    #[serde(default = "default_false")]
    pub disable_keepalive: bool,

    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: u64,

    #[serde(default = "default_zero")]
    pub max_cache_size: u64, // in MB, 0 = unlimited

    #[serde(skip)]
    pub enable_jxl: bool,
    #[serde(skip)]
    pub enable_avif: bool,
    #[serde(skip)]
    pub enable_webp: bool,
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> String { "3333".to_string() }
fn default_quality() -> u8 { 80 }
fn default_img_path() -> String { "./data/pics".to_string() }
fn default_cache_path() -> String { "./data/cache".to_string() }
fn default_crop() -> String { "InterestingAttention".to_string() }
fn default_buffer_size() -> usize { 4096 }
fn default_concurrency() -> usize { 262144 }
fn default_cache_ttl() -> u64 { 2592000 }
fn default_zero() -> u64 { 0 }
fn default_true() -> bool { true }
fn default_false() -> bool { false }

fn default_allowed_types() -> Vec<String> {
    vec![
        "jpg".into(), "png".into(), "jpeg".into(), "gif".into(),
        "bmp".into(), "svg".into(), "heic".into(), "nef".into(),
        "webp".into(), "avif".into(), "jxl".into()
    ]
}

fn default_convert_types() -> Vec<String> {
    vec!["jxl".into(), "avif".into(), "webp".into()]
}

fn deserialize_port<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PortVal {
        Str(String),
        Int(u64),
    }

    match PortVal::deserialize(deserializer)? {
        PortVal::Int(i) => Ok(i.to_string()),
        PortVal::Str(s) => Ok(s),
    }
}

fn deserialize_quality<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum QualityVal {
        Str(String),
        Int(u8),
    }

    match QualityVal::deserialize(deserializer)? {
        QualityVal::Int(i) => Ok(i),
        QualityVal::Str(s) => s.parse::<u8>().map_err(serde::de::Error::custom),
    }
}

impl Default for JxlifyConfig {
    fn default() -> Self {
        let mut cfg = Self {
            host: default_host(),
            port: default_port(),
            quality: default_quality(),
            img_path: default_img_path(),
            cache_path: default_cache_path(),
            img_map: HashMap::new(),
            allowed_types: default_allowed_types(),
            convert_types: default_convert_types(),
            strip_metadata: true,
            enable_extra_params: false,
            crop_interesting: default_crop(),
            read_buffer_size: default_buffer_size(),
            concurrency: default_concurrency(),
            disable_keepalive: false,
            cache_ttl: default_cache_ttl(),
            max_cache_size: 0,
            enable_jxl: true,
            enable_avif: true,
            enable_webp: true,
        };
        cfg.sync_convert_flags();
        cfg
    }
}

impl JxlifyConfig {
    pub fn images_cache_path(&self) -> PathBuf {
        Path::new(&self.cache_path).join("images")
    }

    pub fn metadata_cache_path(&self) -> PathBuf {
        Path::new(&self.cache_path).join("metadata")
    }

    pub fn remote_cache_path(&self) -> PathBuf {
        Path::new(&self.cache_path).join("remote")
    }

    pub fn sync_convert_flags(&mut self) {
        self.enable_jxl = self.convert_types.iter().any(|t| t.eq_ignore_ascii_case("jxl"));
        self.enable_avif = self.convert_types.iter().any(|t| t.eq_ignore_ascii_case("avif"));
        self.enable_webp = self.convert_types.iter().any(|t| t.eq_ignore_ascii_case("webp"));
    }

    pub fn load_from_file_or_default(path_str: &str) -> Self {
        let actual_path = if Path::new(path_str).exists() {
            Some(path_str.to_string())
        } else if path_str == "config.toml" && Path::new("config.json").exists() {
            Some("config.json".to_string())
        } else {
            None
        };

        let mut config = if let Some(ref path) = actual_path {
            match File::open(path) {
                Ok(mut file) => {
                    let mut contents = String::new();
                    if let Ok(_) = file.read_to_string(&mut contents) {
                        let parsed = if path.ends_with(".json") {
                            serde_json::from_str::<JxlifyConfig>(&contents)
                                .map_err(|e| format!("JSON error: {}", e))
                        } else if path.ends_with(".toml") {
                            toml::from_str::<JxlifyConfig>(&contents)
                                .map_err(|e| format!("TOML error: {}", e))
                        } else {
                            toml::from_str::<JxlifyConfig>(&contents)
                                .or_else(|_| serde_json::from_str::<JxlifyConfig>(&contents).map_err(|e| e.to_string()))
                        };

                        match parsed {
                            Ok(cfg) => {
                                info!("Loaded configuration from {}", path);
                                cfg
                            }
                            Err(e) => {
                                warn!("Failed to parse config file {}: {}. Using default config.", path, e);
                                Self::default()
                            }
                        }
                    } else {
                        Self::default()
                    }
                }
                Err(e) => {
                    warn!("Failed to open config file {}: {}. Using default config.", path, e);
                    Self::default()
                }
            }
        } else {
            debug!("Config file {} not found, using defaults and environment variables.", path_str);
            Self::default()
        };

        config.apply_env_overrides();
        config.sync_convert_flags();
        config
    }

    pub fn apply_env_overrides(&mut self) {
        let get_env = |names: &[&str]| -> Option<String> {
            for name in names {
                if let Ok(val) = std::env::var(name) {
                    if !val.trim().is_empty() {
                        return Some(val);
                    }
                }
            }
            None
        };

        if let Some(host) = get_env(&["JXLIFY_HOST"]) {
            self.host = host;
        }
        if let Some(port) = get_env(&["JXLIFY_PORT"]) {
            self.port = port;
        }
        if let Some(path) = get_env(&["JXLIFY_IMG_PATH"]) {
            self.img_path = path;
        }
        if let Some(path) = get_env(&["JXLIFY_CACHE_PATH"]) {
            self.cache_path = path;
        }
        if let Some(q) = get_env(&["JXLIFY_QUALITY"]) {
            if let Ok(parsed) = q.parse::<u8>() {
                self.quality = parsed;
            }
        }
        if let Some(types) = get_env(&["JXLIFY_ALLOWED_TYPES"]) {
            self.allowed_types = types.split(',').map(|s| s.trim().to_lowercase()).collect();
        }
        if let Some(types) = get_env(&["JXLIFY_CONVERT_TYPES"]) {
            self.convert_types = types.split(',').map(|s| s.trim().to_lowercase()).collect();
        }
        if let Some(extra) = get_env(&["JXLIFY_ENABLE_EXTRA_PARAMS"]) {
            self.enable_extra_params = extra.eq_ignore_ascii_case("true") || extra == "1";
        }
        if let Some(crop) = get_env(&["JXLIFY_EXTRA_PARAMS_CROP_INTERESTING"]) {
            self.crop_interesting = crop;
        }
        if let Some(strip) = get_env(&["JXLIFY_STRIP_METADATA"]) {
            self.strip_metadata = strip.eq_ignore_ascii_case("true") || strip == "1";
        }
        if let Some(cache_size) = get_env(&["JXLIFY_MAX_CACHE_SIZE"]) {
            if let Ok(size) = cache_size.parse::<u64>() {
                self.max_cache_size = size;
            }
        }
    }

    pub fn is_allowed_extension(&self, ext: &str) -> bool {
        if self.allowed_types.iter().any(|t| t == "*") {
            return true;
        }
        self.allowed_types.iter().any(|t| t.eq_ignore_ascii_case(ext))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExtraParams {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

impl ExtraParams {
    pub fn is_empty(&self) -> bool {
        self.width.is_none()
            && self.height.is_none()
            && self.max_width.is_none()
            && self.max_height.is_none()
    }
}
