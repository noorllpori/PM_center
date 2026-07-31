use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{codecs::jpeg::JpegEncoder, io::Reader as ImageReader, DynamicImage, Rgb, RgbImage};
use reqwest::{
    header::{
        ACCEPT, ACCEPT_LANGUAGE, CONTENT_LENGTH, CONTENT_TYPE, LOCATION, REFERER, USER_AGENT,
    },
    Client,
};
use scraper::{Html, Selector};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use url::Url;

const HTML_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const IMAGE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const MAX_REDIRECTS: usize = 4;
const CACHE_TTL: Duration = Duration::from_secs(60 * 60);
const CACHE_CAPACITY: usize = 128;
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
const HTML_ACCEPT_VALUE: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
const ACCEPT_LANGUAGE_VALUE: &str = "zh-CN,zh;q=0.9,en;q=0.7";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub description: Option<String>,
    pub site_name: String,
    pub favicon_data_url: Option<String>,
    pub image_data_url: Option<String>,
}

#[derive(Clone)]
struct CachedPreview {
    cached_at: Instant,
    preview: LinkPreview,
}

#[derive(Debug)]
struct FetchedResource {
    final_url: Url,
    content_type: Option<String>,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct ParsedMetadata {
    title: String,
    description: Option<String>,
    site_name: String,
    image_url: Option<Url>,
    favicon_url: Option<Url>,
}

static PREVIEW_CACHE: LazyLock<Mutex<HashMap<String, CachedPreview>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static FETCH_PERMITS: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(4));
static HTML_CHARSET_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?i)charset\s*=\s*[\"']?\s*([a-z0-9._:-]+)"#)
        .expect("valid HTML charset pattern")
});

fn normalize_url(value: &str) -> Result<Url, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 2_048 {
        return Err("网址为空或过长".to_string());
    }
    let candidate = if trimmed.to_ascii_lowercase().starts_with("www.") {
        format!("https://{trimmed}")
    } else {
        trimmed.to_string()
    };
    let mut url = Url::parse(&candidate).map_err(|_| "网址格式无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("只支持 HTTP 或 HTTPS 网址".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("网址不能包含登录凭据".to_string());
    }
    url.set_fragment(None);
    Ok(url)
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4() {
        return is_public_ipv4(ipv4);
    }
    let segments = ip.segments();
    !ip.is_unspecified()
        && !ip.is_loopback()
        && !ip.is_multicast()
        && (segments[0] & 0xfe00) != 0xfc00
        && (segments[0] & 0xffc0) != 0xfe80
        && (segments[0] & 0xffc0) != 0xfec0
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(value) => is_public_ipv4(value),
        IpAddr::V6(value) => is_public_ipv6(value),
    }
}

async fn resolve_public_addresses(url: &Url) -> Result<(String, Vec<SocketAddr>), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "网址缺少主机名".to_string())?
        .trim_end_matches('.')
        .to_string();
    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost"
        || lower_host.ends_with(".localhost")
        || lower_host.ends_with(".local")
    {
        return Err("不允许预览本机或局域网网址".to_string());
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "网址端口无效".to_string())?;
    let mut addresses = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|error| format!("无法解析网址主机：{error}"))?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("不允许预览本机、局域网或保留地址".to_string());
    }
    Ok((host, addresses))
}

async fn fetch_resource(
    initial_url: Url,
    accept: &str,
    max_bytes: usize,
    allow_truncated: bool,
    referer: Option<&Url>,
) -> Result<FetchedResource, String> {
    let mut current = initial_url;
    for redirect_index in 0..=MAX_REDIRECTS {
        let (host, addresses) = resolve_public_addresses(&current).await?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs(&host, &addresses)
            .build()
            .map_err(|error| error.to_string())?;
        let mut request = client
            .get(current.clone())
            .header(USER_AGENT, USER_AGENT_VALUE)
            .header(ACCEPT, accept)
            .header(ACCEPT_LANGUAGE, ACCEPT_LANGUAGE_VALUE);
        if let Some(referer) = referer {
            request = request.header(REFERER, referer.as_str());
        }
        let mut response = request
            .send()
            .await
            .map_err(|error| format!("读取网址失败：{error}"))?;

        if response.status().is_redirection() {
            if redirect_index == MAX_REDIRECTS {
                return Err("网址重定向次数过多".to_string());
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| "网址返回了无效重定向".to_string())?;
            current = current
                .join(location)
                .map_err(|_| "网址返回了无效重定向".to_string())?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("网址返回 HTTP {}", response.status().as_u16()));
        }
        if !allow_truncated
            && response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<usize>().ok())
                .is_some_and(|length| length > max_bytes)
        {
            return Err("网址内容超过预览大小限制".to_string());
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("读取网址内容失败：{error}"))?
        {
            let remaining = max_bytes.saturating_sub(bytes.len());
            if chunk.len() > remaining {
                if allow_truncated {
                    bytes.extend_from_slice(&chunk[..remaining]);
                    break;
                }
                return Err("网址内容超过预览大小限制".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(FetchedResource {
            final_url: current,
            content_type,
            bytes,
        });
    }
    Err("网址重定向次数过多".to_string())
}

fn decode_html(bytes: &[u8], content_type: Option<&str>) -> String {
    if let Some((encoding, bom_length)) = encoding_rs::Encoding::for_bom(bytes) {
        return encoding.decode(&bytes[bom_length..]).0.into_owned();
    }
    let header_charset = content_type.and_then(|value| {
        value.split(';').find_map(|part| {
            let (key, label) = part.trim().split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case("charset")
                .then(|| label.trim().trim_matches(['\'', '"']).to_string())
        })
    });
    let document_charset = String::from_utf8_lossy(&bytes[..bytes.len().min(4_096)]);
    let document_charset = HTML_CHARSET_PATTERN
        .captures(&document_charset)
        .and_then(|captures| captures.get(1))
        .map(|label| label.as_str().to_string());
    if let Some(encoding) = header_charset
        .or(document_charset)
        .as_deref()
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
    {
        return encoding.decode(bytes).0.into_owned();
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn clean_text(value: &str, max_chars: usize) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.chars().take(max_chars).collect())
}

fn meta_content(document: &Html, key: &str, expected: &[&str]) -> Option<String> {
    let selector = Selector::parse("meta").ok()?;
    document.select(&selector).find_map(|element| {
        let value = element.value().attr(key)?;
        if expected
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
        {
            element.value().attr("content").map(str::to_string)
        } else {
            None
        }
    })
}

#[derive(Default)]
struct StructuredMetadata {
    title: Option<String>,
    description: Option<String>,
    site_name: Option<String>,
    image: Option<String>,
}

fn json_value_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(values) => values.iter().find_map(json_value_text),
        serde_json::Value::Object(object) => ["url", "contentUrl", "name"]
            .iter()
            .find_map(|key| object.get(*key).and_then(json_value_text)),
        _ => None,
    }
}

fn find_json_value(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    match value {
        serde_json::Value::Array(values) => {
            values.iter().find_map(|value| find_json_value(value, keys))
        }
        serde_json::Value::Object(object) => keys
            .iter()
            .find_map(|key| object.get(*key).and_then(json_value_text))
            .or_else(|| {
                object
                    .values()
                    .find_map(|value| find_json_value(value, keys))
            }),
        _ => None,
    }
}

fn parse_structured_metadata(document: &Html) -> StructuredMetadata {
    let Some(selector) = Selector::parse("script[type='application/ld+json']").ok() else {
        return StructuredMetadata::default();
    };
    let mut metadata = StructuredMetadata::default();
    for element in document.select(&selector) {
        let json = element.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
            continue;
        };
        metadata.title = metadata
            .title
            .or_else(|| find_json_value(&value, &["headline", "name"]));
        metadata.description = metadata
            .description
            .or_else(|| find_json_value(&value, &["description"]));
        metadata.site_name = metadata.site_name.or_else(|| {
            value
                .get("publisher")
                .and_then(|value| find_json_value(value, &["name"]))
        });
        metadata.image = metadata
            .image
            .or_else(|| find_json_value(&value, &["image", "thumbnailUrl"]));
    }
    metadata
}

fn resolve_document_url(base: &Url, value: Option<String>) -> Option<Url> {
    let value = value?.trim().to_string();
    if value.is_empty() {
        return None;
    }
    base.join(&value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
}

fn parse_document(html: &str, final_url: &Url) -> ParsedMetadata {
    let document = Html::parse_document(html);
    let structured = parse_structured_metadata(&document);
    let base_url = Selector::parse("base[href]")
        .ok()
        .and_then(|selector| {
            document
                .select(&selector)
                .next()
                .and_then(|element| element.value().attr("href"))
                .and_then(|href| final_url.join(href).ok())
        })
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or_else(|| final_url.clone());
    let title = meta_content(&document, "property", &["og:title"])
        .or_else(|| meta_content(&document, "name", &["twitter:title"]))
        .or_else(|| {
            Selector::parse("title").ok().and_then(|selector| {
                document
                    .select(&selector)
                    .next()
                    .map(|element| element.text().collect::<String>())
            })
        })
        .or_else(|| meta_content(&document, "itemprop", &["headline", "name"]))
        .or(structured.title)
        .and_then(|value| clean_text(&value, 160))
        .unwrap_or_else(|| final_url.host_str().unwrap_or("网页").to_string());
    let description = meta_content(&document, "property", &["og:description"])
        .or_else(|| meta_content(&document, "name", &["description", "twitter:description"]))
        .or(structured.description)
        .or_else(|| meta_content(&document, "itemprop", &["description"]))
        .and_then(|value| clean_text(&value, 280));
    let site_name = meta_content(&document, "property", &["og:site_name"])
        .or_else(|| meta_content(&document, "name", &["application-name"]))
        .or(structured.site_name)
        .and_then(|value| clean_text(&value, 80))
        .unwrap_or_else(|| final_url.host_str().unwrap_or("网页").to_string());
    let image_url = resolve_document_url(
        &base_url,
        meta_content(&document, "property", &["og:image", "og:image:url"])
            .or_else(|| meta_content(&document, "name", &["twitter:image"]))
            .or(structured.image)
            .or_else(|| meta_content(&document, "itemprop", &["image", "thumbnailUrl"])),
    );
    let favicon_url = Selector::parse("link[href]")
        .ok()
        .and_then(|selector| {
            document.select(&selector).find_map(|element| {
                let relation = element.value().attr("rel")?;
                relation
                    .split_whitespace()
                    .any(|value| {
                        value.eq_ignore_ascii_case("icon")
                            || value.to_ascii_lowercase().ends_with("-icon")
                    })
                    .then(|| element.value().attr("href").map(str::to_string))
                    .flatten()
            })
        })
        .and_then(|value| resolve_document_url(&base_url, Some(value)))
        .or_else(|| final_url.join("/favicon.ico").ok());
    ParsedMetadata {
        title,
        description,
        site_name,
        image_url,
        favicon_url,
    }
}

fn composite_on_white(image: DynamicImage) -> DynamicImage {
    let rgba = image.to_rgba8();
    let mut output = RgbImage::from_pixel(rgba.width(), rgba.height(), Rgb([255, 255, 255]));
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u16::from(pixel[3]);
        let inverse = 255 - alpha;
        output.put_pixel(
            x,
            y,
            Rgb([
                ((u16::from(pixel[0]) * alpha + 255 * inverse) / 255) as u8,
                ((u16::from(pixel[1]) * alpha + 255 * inverse) / 255) as u8,
                ((u16::from(pixel[2]) * alpha + 255 * inverse) / 255) as u8,
            ]),
        );
    }
    DynamicImage::ImageRgb8(output)
}

async fn fetch_image_data_url(url: Url, favicon: bool, referer: Url) -> Option<String> {
    let resource = fetch_resource(
        url,
        "image/webp,image/png,image/jpeg,image/gif,image/x-icon,image/*;q=0.8,*/*;q=0.5",
        IMAGE_LIMIT_BYTES,
        false,
        Some(&referer),
    )
    .await
    .ok()?;
    let content_type = resource
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !content_type.starts_with("image/")
        && !(favicon
            && (content_type.is_empty() || content_type.starts_with("application/octet-stream")))
    {
        return None;
    }
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(8_192);
    limits.max_image_height = Some(8_192);
    limits.max_alloc = Some(96 * 1024 * 1024);
    let mut reader = ImageReader::new(Cursor::new(&resource.bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let resized = if favicon {
        image.thumbnail(64, 64)
    } else {
        image.thumbnail(640, 360)
    };
    let resized = composite_on_white(resized);
    let mut encoded = Vec::new();
    JpegEncoder::new_with_quality(&mut encoded, if favicon { 88 } else { 78 })
        .encode_image(&resized)
        .ok()?;
    Some(format!("data:image/jpeg;base64,{}", BASE64.encode(encoded)))
}

fn cached_preview(key: &str) -> Option<LinkPreview> {
    let mut cache = PREVIEW_CACHE.lock().ok()?;
    cache.retain(|_, value| value.cached_at.elapsed() < CACHE_TTL);
    cache.get(key).map(|value| value.preview.clone())
}

fn store_preview(key: String, preview: LinkPreview) {
    let Ok(mut cache) = PREVIEW_CACHE.lock() else {
        return;
    };
    cache.retain(|_, value| value.cached_at.elapsed() < CACHE_TTL);
    if cache.len() >= CACHE_CAPACITY {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, value)| value.cached_at)
            .map(|(key, _)| key.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(
        key,
        CachedPreview {
            cached_at: Instant::now(),
            preview,
        },
    );
}

#[tauri::command]
pub async fn get_link_preview(url: String) -> Result<LinkPreview, String> {
    let normalized = normalize_url(&url)?;
    let cache_key = normalized.as_str().to_string();
    if let Some(preview) = cached_preview(&cache_key) {
        return Ok(preview);
    }
    let _permit = FETCH_PERMITS
        .acquire()
        .await
        .map_err(|_| "网址预览服务暂不可用".to_string())?;
    if let Some(preview) = cached_preview(&cache_key) {
        return Ok(preview);
    }
    let page = fetch_resource(
        normalized.clone(),
        HTML_ACCEPT_VALUE,
        HTML_LIMIT_BYTES,
        true,
        None,
    )
    .await?;
    let content_type = page
        .content_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !content_type.is_empty()
        && !content_type.starts_with("text/html")
        && !content_type.starts_with("application/xhtml+xml")
    {
        return Err("网址不是可预览的网页".to_string());
    }
    let html = decode_html(&page.bytes, page.content_type.as_deref());
    let ParsedMetadata {
        title,
        description,
        site_name,
        image_url,
        favicon_url,
    } = parse_document(&html, &page.final_url);
    let (image_data_url, favicon_data_url) = tokio::join!(
        async {
            match image_url {
                Some(url) => fetch_image_data_url(url, false, page.final_url.clone()).await,
                None => None,
            }
        },
        async {
            match favicon_url {
                Some(url) => fetch_image_data_url(url, true, page.final_url.clone()).await,
                None => None,
            }
        }
    );
    let preview = LinkPreview {
        url: normalized.to_string(),
        final_url: page.final_url.to_string(),
        title,
        description,
        site_name,
        favicon_data_url,
        image_data_url,
    };
    store_preview(cache_key, preview.clone());
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_reserved_addresses_are_rejected() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.1.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.1.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
        ] {
            assert!(!is_public_ipv4(value.parse().unwrap()), "{value}");
        }
        assert!(is_public_ipv4("8.8.8.8".parse().unwrap()));
        assert!(!is_public_ipv6(Ipv6Addr::LOCALHOST));
        assert!(is_public_ipv6("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn open_graph_metadata_and_relative_assets_are_parsed() {
        let html = r#"
          <html><head>
            <meta property="og:title" content="  Example   title ">
            <meta property="og:description" content="Description">
            <meta property="og:site_name" content="Example">
            <meta property="og:image" content="/cover.png">
            <link rel="shortcut icon" href="icons/favicon.png">
          </head></html>
        "#;
        let base = Url::parse("https://example.com/article/1").unwrap();
        let parsed = parse_document(html, &base);
        assert_eq!(parsed.title, "Example title");
        assert_eq!(parsed.description.as_deref(), Some("Description"));
        assert_eq!(
            parsed.image_url.unwrap().as_str(),
            "https://example.com/cover.png"
        );
        assert_eq!(
            parsed.favicon_url.unwrap().as_str(),
            "https://example.com/article/icons/favicon.png"
        );
    }

    #[test]
    fn only_http_urls_without_credentials_are_accepted() {
        assert!(normalize_url("https://example.com/path#fragment").is_ok());
        assert!(normalize_url("www.example.com").is_ok());
        assert!(normalize_url("file:///etc/passwd").is_err());
        assert!(normalize_url("https://user:password@example.com").is_err());
    }

    #[test]
    fn document_charset_is_used_when_the_header_has_none() {
        let html = r#"<meta charset="gb2312"><title>中文标题</title>"#;
        let (encoded, _, _) = encoding_rs::GBK.encode(html);
        assert!(decode_html(&encoded, Some("text/html")).contains("中文标题"));
    }

    #[test]
    fn json_ld_fills_missing_page_metadata() {
        let html = r#"
          <script type="application/ld+json">
            {
              "@type": "Article",
              "headline": "Structured title",
              "description": "Structured description",
              "image": "/structured-cover.jpg",
              "publisher": { "name": "Structured site" }
            }
          </script>
        "#;
        let base = Url::parse("https://example.com/article/1").unwrap();
        let parsed = parse_document(html, &base);
        assert_eq!(parsed.title, "Structured title");
        assert_eq!(
            parsed.description.as_deref(),
            Some("Structured description")
        );
        assert_eq!(parsed.site_name, "Structured site");
        assert_eq!(
            parsed.image_url.unwrap().as_str(),
            "https://example.com/structured-cover.jpg"
        );
    }
}
