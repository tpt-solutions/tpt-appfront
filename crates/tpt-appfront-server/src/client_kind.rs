//! Client-type detection via User-Agent header and `?client=` query parameter.

/// The kind of client making the request.
///
/// Determines which rendering backend serves the response:
/// - [`Human`](ClientKind::Human) → WASM+DOM or Canvas shell (interactive app).
/// - [`Crawler`](ClientKind::Crawler) → semantic HTML via `tpt-appfront-html`.
/// - [`AiAgent`](ClientKind::AiAgent) → JSON-LD + AI Schema via `tpt-appfront-ai-schema`.
/// - [`SocialBot`](ClientKind::SocialBot) → OpenGraph meta tags via `tpt-appfront-html`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    Human,
    Crawler,
    AiAgent,
    SocialBot,
}

/// Known crawler User-Agent substrings.
const CRAWLER_AGENTS: &[&str] = &[
    "googlebot",
    "bingbot",
    "yandexbot",
    "duckduckbot",
    "baiduspider",
    "facebot",
    "facebookexternalhit",
    "slackbot",
    "twitterbot",
    "discordbot",
    "telegrambot",
    "whatsapp",
    "applebot",
    "semrushbot",
    "ahrefsbot",
    "dotbot",
    "seznambot",
    "sogou",
    "exabot",
    "mj12bot",
    "yeti",
];

/// Known AI-agent User-Agent substrings.
const AI_AGENTS: &[&str] = &[
    "gptbot",
    "claude",
    "anthropic",
    "perplexitybot",
    "cohere",
    "google-ai",
];

/// Known social-media bot User-Agent substrings (those that care about
/// OpenGraph but aren't general-purpose crawlers).
const SOCIAL_BOTS: &[&str] = &[
    "facebookexternalhit",
    "twitterbot",
    "slackbot",
    "discordbot",
    "telegrambot",
    "whatsapp",
    "pinterest",
];

/// Detect the client kind from the `User-Agent` header value.
///
/// The `?client=` query parameter can override detection (values: `human`,
/// `crawler`, `ai`, `social`).
pub fn detect(ua: Option<&str>, query_client: Option<&str>) -> ClientKind {
    // Query-param override takes precedence.
    if let Some(override_val) = query_client {
        match override_val.to_ascii_lowercase().as_str() {
            "human" | "browser" | "web" => return ClientKind::Human,
            "crawler" | "bot" | "seo" => return ClientKind::Crawler,
            "ai" | "agent" | "aiagent" => return ClientKind::AiAgent,
            "social" | "socialbot" | "opengraph" => return ClientKind::SocialBot,
            _ => {}
        }
    }

    let Some(ua) = ua else {
        return ClientKind::Human;
    };

    let ua_lower = ua.to_ascii_lowercase();

    // Check social bots first (they're a subset of crawlers).
    if contains_any(&ua_lower, SOCIAL_BOTS) {
        return ClientKind::SocialBot;
    }

    if contains_any(&ua_lower, AI_AGENTS) {
        return ClientKind::AiAgent;
    }

    if contains_any(&ua_lower, CRAWLER_AGENTS) {
        return ClientKind::Crawler;
    }

    ClientKind::Human
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| text.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_browser_is_human() {
        assert_eq!(
            detect(Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120"), None),
            ClientKind::Human
        );
    }

    #[test]
    fn googlebot_is_crawler() {
        assert_eq!(
            detect(Some("Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"), None),
            ClientKind::Crawler
        );
    }

    #[test]
    fn twitterbot_is_social() {
        assert_eq!(
            detect(Some("Twitterbot/1.0"), None),
            ClientKind::SocialBot
        );
    }

    #[test]
    fn gptbot_is_ai() {
        assert_eq!(
            detect(Some("Mozilla/5.0 (compatible; GPTBot/1.0)"), None),
            ClientKind::AiAgent
        );
    }

    #[test]
    fn claude_is_ai() {
        assert_eq!(
            detect(Some("Claude-Web"), None),
            ClientKind::AiAgent
        );
    }

    #[test]
    fn no_ua_is_human() {
        assert_eq!(detect(None, None), ClientKind::Human);
    }

    #[test]
    fn query_param_overrides() {
        assert_eq!(
            detect(Some("Googlebot/2.1"), Some("human")),
            ClientKind::Human
        );
        assert_eq!(
            detect(Some("Mozilla/5.0 Chrome/120"), Some("crawler")),
            ClientKind::Crawler
        );
        assert_eq!(
            detect(Some("Mozilla/5.0 Chrome/120"), Some("ai")),
            ClientKind::AiAgent
        );
        assert_eq!(
            detect(Some("Googlebot/2.1"), Some("social")),
            ClientKind::SocialBot
        );
    }

    #[test]
    fn empty_ua_is_human() {
        assert_eq!(detect(Some(""), None), ClientKind::Human);
    }

    #[test]
    fn discordbot_is_social() {
        assert_eq!(
            detect(Some("Discordbot/2.0; +https://discordapp.com"), None),
            ClientKind::SocialBot
        );
    }
}
