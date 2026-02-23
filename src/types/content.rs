//! Content block types used within Claude messages.
//!
//! Claude's protocol carries heterogeneous content as a tagged union. This
//! module provides the [`ContentBlock`] enum (used inside assistant messages)
//! and [`UserContent`] (used when constructing user turns).
//!
//! # Serde representation
//!
//! Both enums use an *externally-tagged* representation via
//! `#[serde(tag = "type", rename_all = "snake_case")]`, matching the
//! NDJSON format produced by the Claude Code CLI.
//!
//! # Example
//!
//! ```rust
//! use claude_cli_sdk::types::content::{UserContent, ContentBlock};
//!
//! let msg = UserContent::text("Hello, Claude!");
//! let with_image = UserContent::image_url("https://example.com/img.png", "image/png");
//! ```

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum allowed size for a base64-encoded image payload (15 MiB).
pub const MAX_IMAGE_BASE64_BYTES: usize = 15 * 1024 * 1024;

/// MIME types accepted by the Claude API for image inputs.
pub const ALLOWED_IMAGE_MIME_TYPES: &[&str] =
    &["image/jpeg", "image/png", "image/gif", "image/webp"];

// ── ContentBlock ──────────────────────────────────────────────────────────────

/// A single block of content within an assistant message.
///
/// The Claude CLI serialises each block as a JSON object with a `"type"` field.
/// Unknown future types are preserved verbatim via the `#[serde(other)]` variant
/// on deserialization — but since `serde(other)` cannot carry data on tagged
/// enums we instead capture unknowns through the `extra` fields on parent
/// structs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain-text content produced by the model.
    Text(TextBlock),

    /// A tool invocation requested by the model.
    ToolUse(ToolUseBlock),

    /// The result of a tool invocation, sent back as user content.
    ToolResult(ToolResultBlock),

    /// Extended thinking content (chain-of-thought).
    Thinking(ThinkingBlock),

    /// An image (base64 or URL).
    Image(ImageBlock),
}

impl ContentBlock {
    /// Returns the text string if this block is [`ContentBlock::Text`].
    #[inline]
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(b) => Some(&b.text),
            _ => None,
        }
    }

    /// Returns `true` if this is a [`ContentBlock::ToolUse`] block.
    #[inline]
    #[must_use]
    pub fn is_tool_use(&self) -> bool {
        matches!(self, Self::ToolUse(_))
    }
}

// ── Text block ────────────────────────────────────────────────────────────────

/// A plain-text content block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    /// The text produced by the model.
    pub text: String,
}

// ── Tool-use block ────────────────────────────────────────────────────────────

/// A tool invocation requested by the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUseBlock {
    /// Unique identifier for this particular tool call (used to correlate
    /// with the corresponding [`ToolResultBlock`]).
    pub id: String,

    /// Name of the tool being invoked.
    pub name: String,

    /// JSON-encoded arguments passed to the tool.
    #[serde(default)]
    pub input: serde_json::Value,
}

// ── Tool-result block ─────────────────────────────────────────────────────────

/// The result of a tool invocation, returned as part of a user turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultBlock {
    /// The `id` from the corresponding [`ToolUseBlock`].
    pub tool_use_id: String,

    /// Whether the tool invocation produced an error.
    #[serde(default)]
    pub is_error: bool,

    /// Content returned by the tool.  May be text or nested content blocks.
    #[serde(default)]
    pub content: ToolResultContent,
}

/// Either a plain string or a list of content blocks — both are valid in the
/// protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// Simple text result.
    Text(String),
    /// Richer multi-block result.
    Blocks(Vec<ContentBlock>),
}

impl Default for ToolResultContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

// ── Thinking block ────────────────────────────────────────────────────────────

/// Extended thinking produced by the model (chain-of-thought).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// The chain-of-thought text.
    pub thinking: String,

    /// Opaque signature produced by the API (used for verification).
    #[serde(default)]
    pub signature: Option<String>,
}

// ── Image block ───────────────────────────────────────────────────────────────

/// An image content block, supporting both base64-encoded and URL sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageBlock {
    /// The image source.
    pub source: ImageSource,
}

/// The source of an image — either inline base64 data or a remote URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// A base64-encoded image with an explicit MIME type.
    Base64(Base64ImageSource),
    /// A URL pointing to a publicly accessible image.
    Url(UrlImageSource),
}

/// Inline base64 image data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Base64ImageSource {
    /// MIME type, e.g. `"image/png"`.
    pub media_type: String,
    /// Base64-encoded image bytes (no `data:` prefix).
    pub data: String,
}

/// A URL-referenced image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlImageSource {
    /// The image URL.
    pub url: String,
    /// MIME type hint, e.g. `"image/png"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

// ── UserContent ───────────────────────────────────────────────────────────────

/// Content that can be sent as part of a user turn.
///
/// This is the primary type callers use to build messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserContent {
    /// A plain text message.
    Text(TextBlock),
    /// An image (base64 or URL).
    Image(ImageBlock),
}

impl UserContent {
    /// Construct a plain-text [`UserContent`] from any string-like value.
    ///
    /// # Example
    ///
    /// ```rust
    /// use claude_cli_sdk::types::content::UserContent;
    /// let msg = UserContent::text("Hello, Claude!");
    /// ```
    #[inline]
    #[must_use]
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(TextBlock { text: s.into() })
    }

    /// Construct a base64-encoded image [`UserContent`], validating the MIME
    /// type and payload size.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ImageValidation`] if:
    /// - `media_type` is not one of `image/jpeg`, `image/png`, `image/gif`,
    ///   `image/webp`.
    /// - `data` (the base64 string) exceeds 15 MiB.
    ///
    /// # Example
    ///
    /// ```rust
    /// use claude_cli_sdk::types::content::UserContent;
    ///
    /// let content = UserContent::image_base64("aGVsbG8=", "image/png").unwrap();
    /// ```
    pub fn image_base64(data: impl Into<String>, media_type: impl Into<String>) -> Result<Self> {
        let data = data.into();
        let media_type = media_type.into();

        validate_mime_type(&media_type)?;
        validate_base64_size(&data)?;

        Ok(Self::Image(ImageBlock {
            source: ImageSource::Base64(Base64ImageSource { media_type, data }),
        }))
    }

    /// Construct a URL-referenced image [`UserContent`], validating the MIME
    /// type hint when provided.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ImageValidation`] if `media_type` is `Some` but not in
    /// the allowed MIME type list.
    ///
    /// # Example
    ///
    /// ```rust
    /// use claude_cli_sdk::types::content::UserContent;
    ///
    /// let content = UserContent::image_url("https://example.com/img.png", "image/png").unwrap();
    /// ```
    pub fn image_url(url: impl Into<String>, media_type: impl Into<String>) -> Result<Self> {
        let media_type = media_type.into();
        validate_mime_type(&media_type)?;

        Ok(Self::Image(ImageBlock {
            source: ImageSource::Url(UrlImageSource {
                url: url.into(),
                media_type: Some(media_type),
            }),
        }))
    }

    /// Construct a URL-referenced image without a MIME type hint.
    #[inline]
    #[must_use]
    pub fn image_url_untyped(url: impl Into<String>) -> Self {
        Self::Image(ImageBlock {
            source: ImageSource::Url(UrlImageSource {
                url: url.into(),
                media_type: None,
            }),
        })
    }

    /// Returns the text string if this is a [`UserContent::Text`] variant.
    #[inline]
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(b) => Some(&b.text),
            _ => None,
        }
    }
}

// ── From<&str> / From<String> ─────────────────────────────────────────────────

impl From<&str> for UserContent {
    #[inline]
    fn from(s: &str) -> Self {
        Self::text(s)
    }
}

impl From<String> for UserContent {
    #[inline]
    fn from(s: String) -> Self {
        Self::text(s)
    }
}

// ── Validation helpers ────────────────────────────────────────────────────────

/// Validates that `media_type` is in the allowed MIME type list.
fn validate_mime_type(media_type: &str) -> Result<()> {
    if ALLOWED_IMAGE_MIME_TYPES.contains(&media_type) {
        Ok(())
    } else {
        Err(Error::ImageValidation(format!(
            "unsupported MIME type '{media_type}'; allowed: {}",
            ALLOWED_IMAGE_MIME_TYPES.join(", ")
        )))
    }
}

/// Validates that a base64 data string does not exceed [`MAX_IMAGE_BASE64_BYTES`].
fn validate_base64_size(data: &str) -> Result<()> {
    if data.len() > MAX_IMAGE_BASE64_BYTES {
        Err(Error::ImageValidation(format!(
            "base64 image data exceeds the 15 MiB limit ({} bytes)",
            data.len()
        )))
    } else {
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── UserContent::text ─────────────────────────────────────────────────────

    #[test]
    fn user_content_text_round_trip() {
        let original = UserContent::text("Hello!");
        let json = serde_json::to_string(&original).unwrap();
        let decoded: UserContent = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn user_content_text_serde_shape() {
        let c = UserContent::text("hi");
        let v: serde_json::Value = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "text");
        assert_eq!(v["text"], "hi");
    }

    // ── From<&str> / From<String> ─────────────────────────────────────────────

    #[test]
    fn from_str_produces_text_variant() {
        let c: UserContent = "hello".into();
        assert_eq!(c.as_text(), Some("hello"));
    }

    #[test]
    fn from_string_produces_text_variant() {
        let c: UserContent = String::from("world").into();
        assert_eq!(c.as_text(), Some("world"));
    }

    // ── UserContent::image_base64 ─────────────────────────────────────────────

    #[test]
    fn image_base64_valid_mime_types() {
        for mime in ALLOWED_IMAGE_MIME_TYPES {
            let result = UserContent::image_base64("aGVsbG8=", *mime);
            assert!(result.is_ok(), "should accept {mime}");
        }
    }

    #[test]
    fn image_base64_rejects_unsupported_mime() {
        let err = UserContent::image_base64("aGVsbG8=", "image/bmp").unwrap_err();
        assert!(
            matches!(err, Error::ImageValidation(_)),
            "expected ImageValidation, got {err:?}"
        );
        assert!(err.to_string().contains("image/bmp"));
    }

    #[test]
    fn image_base64_rejects_oversized_payload() {
        // 15 MiB + 1 byte of 'A' characters exceeds the limit.
        let oversized = "A".repeat(MAX_IMAGE_BASE64_BYTES + 1);
        let err = UserContent::image_base64(oversized, "image/png").unwrap_err();
        assert!(matches!(err, Error::ImageValidation(_)));
        assert!(err.to_string().contains("15 MiB"));
    }

    #[test]
    fn image_base64_accepts_exactly_at_limit() {
        let at_limit = "A".repeat(MAX_IMAGE_BASE64_BYTES);
        let result = UserContent::image_base64(at_limit, "image/png");
        assert!(result.is_ok());
    }

    #[test]
    fn image_base64_round_trip() {
        let original = UserContent::image_base64("aGVsbG8=", "image/jpeg").unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: UserContent = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn image_base64_serde_shape() {
        let c = UserContent::image_base64("abc123", "image/png").unwrap();
        let v: serde_json::Value = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "image");
        assert_eq!(v["source"]["type"], "base64");
        assert_eq!(v["source"]["media_type"], "image/png");
        assert_eq!(v["source"]["data"], "abc123");
    }

    // ── UserContent::image_url ────────────────────────────────────────────────

    #[test]
    fn image_url_valid() {
        let result = UserContent::image_url("https://example.com/img.png", "image/png");
        assert!(result.is_ok());
    }

    #[test]
    fn image_url_rejects_bad_mime() {
        let err =
            UserContent::image_url("https://example.com/img.svg", "image/svg+xml").unwrap_err();
        assert!(matches!(err, Error::ImageValidation(_)));
    }

    #[test]
    fn image_url_round_trip() {
        let original = UserContent::image_url("https://example.com/img.gif", "image/gif").unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: UserContent = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn image_url_serde_shape() {
        let c = UserContent::image_url("https://example.com/a.webp", "image/webp").unwrap();
        let v: serde_json::Value = serde_json::to_value(&c).unwrap();
        assert_eq!(v["type"], "image");
        assert_eq!(v["source"]["type"], "url");
        assert_eq!(v["source"]["url"], "https://example.com/a.webp");
    }

    #[test]
    fn image_url_untyped_no_media_type_field() {
        let c = UserContent::image_url_untyped("https://example.com/a.png");
        let v: serde_json::Value = serde_json::to_value(&c).unwrap();
        assert!(
            v["source"]["media_type"].is_null(),
            "media_type should be omitted"
        );
    }

    // ── ContentBlock ──────────────────────────────────────────────────────────

    #[test]
    fn content_block_text_round_trip() {
        let block = ContentBlock::Text(TextBlock {
            text: "response".into(),
        });
        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, decoded);
    }

    #[test]
    fn content_block_text_serde_shape() {
        let block = ContentBlock::Text(TextBlock {
            text: "hello".into(),
        });
        let v: serde_json::Value = serde_json::to_value(&block).unwrap();
        assert_eq!(v["type"], "text");
        assert_eq!(v["text"], "hello");
    }

    #[test]
    fn content_block_tool_use_round_trip() {
        let block = ContentBlock::ToolUse(ToolUseBlock {
            id: "call_123".into(),
            name: "bash".into(),
            input: serde_json::json!({ "command": "ls" }),
        });
        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, decoded);
    }

    #[test]
    fn content_block_tool_use_serde_shape() {
        let block = ContentBlock::ToolUse(ToolUseBlock {
            id: "id1".into(),
            name: "read_file".into(),
            input: serde_json::json!({ "path": "/tmp/foo" }),
        });
        let v: serde_json::Value = serde_json::to_value(&block).unwrap();
        assert_eq!(v["type"], "tool_use");
        assert_eq!(v["name"], "read_file");
    }

    #[test]
    fn content_block_tool_result_round_trip() {
        let block = ContentBlock::ToolResult(ToolResultBlock {
            tool_use_id: "call_123".into(),
            is_error: false,
            content: ToolResultContent::Text("file contents".into()),
        });
        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, decoded);
    }

    #[test]
    fn content_block_thinking_round_trip() {
        let block = ContentBlock::Thinking(ThinkingBlock {
            thinking: "Let me think...".into(),
            signature: Some("sig123".into()),
        });
        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, decoded);
    }

    #[test]
    fn content_block_as_text_helper() {
        let text = ContentBlock::Text(TextBlock {
            text: "hello".into(),
        });
        assert_eq!(text.as_text(), Some("hello"));

        let tool = ContentBlock::ToolUse(ToolUseBlock {
            id: "x".into(),
            name: "bash".into(),
            input: serde_json::Value::Null,
        });
        assert_eq!(tool.as_text(), None);
    }

    #[test]
    fn content_block_is_tool_use_helper() {
        let tool = ContentBlock::ToolUse(ToolUseBlock {
            id: "x".into(),
            name: "bash".into(),
            input: serde_json::Value::Null,
        });
        assert!(tool.is_tool_use());
        assert!(!ContentBlock::Text(TextBlock { text: "hi".into() }).is_tool_use());
    }

    #[test]
    fn tool_result_content_default_is_empty_text() {
        let default = ToolResultContent::default();
        assert_eq!(default, ToolResultContent::Text(String::new()));
    }

    #[test]
    fn image_block_round_trip() {
        let block = ContentBlock::Image(ImageBlock {
            source: ImageSource::Base64(Base64ImageSource {
                media_type: "image/png".into(),
                data: "abc==".into(),
            }),
        });
        let json = serde_json::to_string(&block).unwrap();
        let decoded: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, decoded);
    }
}
