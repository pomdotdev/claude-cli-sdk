//! Type definitions for the claude-cli-sdk.
//!
//! This module re-exports all public types from the two submodules:
//!
//! - [`content`]: Content block types ([`ContentBlock`], [`UserContent`], …)
//! - [`messages`]: Top-level NDJSON message types ([`Message`], [`Usage`], …)

pub mod content;
pub mod messages;

pub use content::{
    ALLOWED_IMAGE_MIME_TYPES, Base64ImageSource, ContentBlock, ImageBlock, ImageSource,
    MAX_IMAGE_BASE64_BYTES, TextBlock, ThinkingBlock, ToolResultBlock, ToolResultContent,
    ToolUseBlock, UrlImageSource, UserContent,
};

pub use messages::{
    AssistantMessage, AssistantMessageInner, McpServerStatus, Message, ResultMessage, SessionInfo,
    StreamEvent, SystemMessage, Usage, UserMessage, UserMessageInner,
};
