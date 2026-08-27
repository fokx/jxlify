#![allow(dead_code)]
use crate::config::JxlifyConfig;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiatedFormat {
    Jxl,
    Avif,
    Webp,
    Raw,
}

impl fmt::Display for NegotiatedFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NegotiatedFormat::Jxl => write!(f, "jxl"),
            NegotiatedFormat::Avif => write!(f, "avif"),
            NegotiatedFormat::Webp => write!(f, "webp"),
            NegotiatedFormat::Raw => write!(f, "raw"),
        }
    }
}

impl NegotiatedFormat {
    pub fn mime_type(&self) -> &'static str {
        match self {
            NegotiatedFormat::Jxl => "image/jxl",
            NegotiatedFormat::Avif => "image/avif",
            NegotiatedFormat::Webp => "image/webp",
            NegotiatedFormat::Raw => "application/octet-stream",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            NegotiatedFormat::Jxl => "jxl",
            NegotiatedFormat::Avif => "avif",
            NegotiatedFormat::Webp => "webp",
            NegotiatedFormat::Raw => "raw",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClientCapabilities {
    pub jxl: bool,
    pub avif: bool,
    pub webp: bool,
}

/// Parse Accept header and User-Agent to determine client format capabilities
pub fn detect_client_capabilities(accept: Option<&str>, user_agent: Option<&str>) -> ClientCapabilities {
    let mut caps = ClientCapabilities::default();

    // 1. Inspect Accept header
    if let Some(accept_str) = accept {
        let accept_lower = accept_str.to_lowercase();
        if accept_lower.contains("image/jxl") {
            caps.jxl = true;
        }
        if accept_lower.contains("image/avif") {
            caps.avif = true;
        }
        if accept_lower.contains("image/webp") {
            caps.webp = true;
        }
    }

    // 2. Inspect User-Agent heuristic (for direct address bar navigations or clients with */*)
    if let Some(ua_str) = user_agent {
        let ua_lower = ua_str.to_lowercase();

        // Check browsers with built-in or default JPEG XL support
        if ua_lower.contains("thorium")
            || ua_lower.contains("floorp")
            || ua_lower.contains("zen/")
            || ua_lower.contains("palemoon")
            || ua_lower.contains("waterfox")
            || ua_lower.contains("basilisk")
        {
            caps.jxl = true;
            caps.avif = true;
            caps.webp = true;
        }

        // Check iOS / Safari versions
        if let Some(version) = extract_version_after_patterns(&ua_lower, &["version/", "os "]) {
            if version >= 17 {
                caps.jxl = true;
                caps.avif = true;
                caps.webp = true;
            } else if version >= 16 {
                caps.avif = true;
                caps.webp = true;
            } else if version >= 14 {
                caps.webp = true;
            }
        }

        // Check Chrome / Chromium versions
        if let Some(chrome_ver) = extract_version_after_patterns(&ua_lower, &["chrome/", "crios/"]) {
            if chrome_ver >= 85 {
                caps.avif = true;
                caps.webp = true;
            } else if chrome_ver >= 32 {
                caps.webp = true;
            }
        }

        // Check Firefox versions
        if let Some(ff_ver) = extract_version_after_patterns(&ua_lower, &["firefox/", "fxios/"]) {
            if ff_ver >= 93 {
                caps.avif = true;
                caps.webp = true;
            } else if ff_ver >= 65 {
                caps.webp = true;
            }
        }
    }

    caps
}

/// Negotiate the single optimal target format for the client
pub fn negotiate_format(
    accept: Option<&str>,
    user_agent: Option<&str>,
    config: &JxlifyConfig,
) -> NegotiatedFormat {
    let caps = detect_client_capabilities(accept, user_agent);

    // Strict priority hierarchy: JXL -> AVIF -> WebP -> Raw
    if config.enable_jxl && caps.jxl {
        NegotiatedFormat::Jxl
    } else if config.enable_avif && caps.avif {
        NegotiatedFormat::Avif
    } else if config.enable_webp && caps.webp {
        NegotiatedFormat::Webp
    } else {
        NegotiatedFormat::Raw
    }
}

/// Helper to parse major browser/OS versions from user-agent strings
fn extract_version_after_patterns(ua: &str, patterns: &[&str]) -> Option<u32> {
    for pattern in patterns {
        if let Some(pos) = ua.find(pattern) {
            let sub = &ua[pos + pattern.len()..];
            let ver_str: String = sub.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(num) = ver_str.parse::<u32>() {
                return Some(num);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiation_accept_header() {
        let config = JxlifyConfig::default();

        let fmt_jxl = negotiate_format(Some("image/jxl,image/avif,image/webp"), None, &config);
        assert_eq!(fmt_jxl, NegotiatedFormat::Jxl);

        let fmt_avif = negotiate_format(Some("image/avif,image/webp,*/*;q=0.8"), None, &config);
        assert_eq!(fmt_avif, NegotiatedFormat::Avif);

        let fmt_webp = negotiate_format(Some("image/webp,image/apng,*/*;q=0.8"), None, &config);
        assert_eq!(fmt_webp, NegotiatedFormat::Webp);

        let fmt_raw = negotiate_format(Some("image/jpeg,image/png"), None, &config);
        assert_eq!(fmt_raw, NegotiatedFormat::Raw);
    }

    #[test]
    fn test_negotiation_user_agent_heuristic() {
        let config = JxlifyConfig::default();

        // Safari 17 on iOS
        let ua_safari17 = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1";
        let fmt = negotiate_format(None, Some(ua_safari17), &config);
        assert_eq!(fmt, NegotiatedFormat::Jxl);

        // Thorium
        let ua_thorium = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36 Thorium/122.0.6261.128";
        let fmt = negotiate_format(None, Some(ua_thorium), &config);
        assert_eq!(fmt, NegotiatedFormat::Jxl);

        // Floorp
        let ua_floorp = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0 Floorp/11.17.0";
        let fmt = negotiate_format(None, Some(ua_floorp), &config);
        assert_eq!(fmt, NegotiatedFormat::Jxl);

        // Zen Browser
        let ua_zen = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:130.0) Gecko/20100101 Firefox/130.0 Zen/1.0.0";
        let fmt = negotiate_format(None, Some(ua_zen), &config);
        assert_eq!(fmt, NegotiatedFormat::Jxl);

        // Chrome 120
        let ua_chrome = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let fmt = negotiate_format(None, Some(ua_chrome), &config);
        assert_eq!(fmt, NegotiatedFormat::Avif);

        // Firefox 130
        let ua_firefox = "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0";
        let fmt = negotiate_format(None, Some(ua_firefox), &config);
        assert_eq!(fmt, NegotiatedFormat::Avif);
    }
}
