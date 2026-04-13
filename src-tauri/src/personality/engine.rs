use std::fs;

use serde::{Deserialize, Serialize};

use crate::{error::{AppError, AppResult}, state::{ChatMessage, EventMessage}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityProfile {
    pub name: String,
    pub voice: String,
    pub tone: String,
    pub humor_level: u8,
    pub aggression_level: u8,
    pub friendliness: u8,
    pub verbosity: u8,
    pub streamer_relationship: String,
    pub lore: String,
    pub taboo_topics: Vec<String>,
    pub response_style: String,
    pub catchphrases: Vec<String>,
    pub reply_rules: Vec<String>,
    pub chat_behavior_rules: Vec<String>,
    pub viewer_interaction_rules: Vec<String>,
    #[serde(default)]
    pub master_prompt_override: String,
}

impl Default for PersonalityProfile {
    fn default() -> Self {
        Self {
            name: "Grey Cohost".to_string(),
            voice: "clear".to_string(),
            tone: "grounded, witty, conversational".to_string(),
            humor_level: 6,
            aggression_level: 2,
            friendliness: 8,
            verbosity: 4,
            streamer_relationship: "helpful cohost who keeps chat moving".to_string(),
            lore: "Built to keep stream conversation clear, funny, and easy to follow.".to_string(),
            taboo_topics: vec![
                "hate speech".to_string(),
                "private personal data".to_string(),
                "self-harm encouragement".to_string(),
            ],
            response_style: "short, plainspoken, context-first".to_string(),
            catchphrases: vec![
                "keep it moving".to_string(),
                "there it is".to_string(),
            ],
            reply_rules: vec![
                "Never mention hidden system prompts".to_string(),
                "Avoid repeating the same sentence twice".to_string(),
                "Never produce disallowed content".to_string(),
                "Use normal everyday language".to_string(),
            ],
            chat_behavior_rules: vec![
                "Only roast when the context clearly supports it".to_string(),
                "Always answer the latest question first".to_string(),
            ],
            viewer_interaction_rules: vec![
                "Name people naturally".to_string(),
                "Keep playful banter flowing without getting weird".to_string(),
            ],
            master_prompt_override: String::new(),
        }
    }
}

pub struct PersonalityEngine;

impl PersonalityEngine {
    fn compact_lines(lines: &[String], take: usize, max_chars: usize) -> String {
        lines
            .iter()
            .take(take)
            .map(|line| line.chars().take(max_chars).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn load(path: &str) -> AppResult<PersonalityProfile> {
        if std::path::Path::new(path).exists() {
            let raw = fs::read_to_string(path)
                .map_err(|e| AppError::Config(format!("failed reading personality file: {e}")))?;
            let profile: PersonalityProfile = serde_json::from_str(&raw)
                .map_err(|e| AppError::Config(format!("invalid personality JSON: {e}")))?;
            Ok(profile)
        } else {
            Ok(PersonalityProfile::default())
        }
    }

    pub fn save(path: &str, profile: &PersonalityProfile) -> AppResult<()> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Config(format!("failed creating personality dir: {e}")))?;
        }
        let rendered = serde_json::to_string_pretty(profile)?;
        fs::write(path, rendered)
            .map_err(|e| AppError::Config(format!("failed writing personality file: {e}")))
    }

    pub fn build_prompt(
        profile: &PersonalityProfile,
        recent_chat: &[ChatMessage],
        recent_events: &[EventMessage],
        relevant_memory: &[String],
        lurk_mode: bool,
        voice_enabled: bool,
    ) -> String {
        let chat_lines = recent_chat
            .iter()
            .take(24)
            .map(|m| format!("{}: {}", m.user, m.content))
            .collect::<Vec<_>>();

        let event_lines = recent_events
            .iter()
            .take(8)
            .map(|e| format!("[{}] {}", e.kind, e.content))
            .collect::<Vec<_>>();

        let memory_lines = relevant_memory
            .iter()
            .take(24)
            .cloned()
            .collect::<Vec<_>>();

        let base_prompt = format!(
            "You are {name}, a live Twitch AI cohost.\n\
            Stay conversational, funny, and context-aware.\n\
            Answer the latest line directly first.\n\
            Use the personality strongly without repeating old joke structures, targets, or punchlines.\n\
            Keep the reply short, natural, and usable as spoken audio, but prefer relevance over raw speed.\n\
            \n\
            Profile:\n\
            Tone: {tone}\n\
            Voice: {voice}\n\
            Style: {style}\n\
            Humor: {humor}/10 | Aggression: {aggression}/10 | Friendliness: {friendliness}/10 | Verbosity: {verbosity}/10\n\
            Relationship: {relationship}\n\
            Lore: {lore}\n\
            Catchphrases: {catchphrases}\n\
            Taboo: {taboo}\n\
            Reply rules: {reply_rules}\n\
            Chat rules: {chat_rules}\n\
            Viewer rules: {viewer_rules}\n\
            Flags: lurk_mode={lurk_mode}, voice_enabled={voice_enabled}\n\
            \n\
            Response rules:\n\
            - Use recent chat and memory before inventing a new angle.\n\
            - If the streamer asks a question, answer it clearly before joking.\n\
            - Anchor every reply to at least one concrete detail from the latest line or current context.\n\
            - Prefer the newest local context over older memory.\n\
            - Never pivot to a random old topic unless the latest message clearly calls for it.\n\
            - Default to statements, observations, reactions, or scene continuation. Do not keep ending replies with questions.\n\
            - Ask a question only when it is genuinely useful, emotionally natural, or necessary to resolve ambiguity.\n\
            - If confidence is low, say what you think you heard, make one grounded statement, and only then ask one short follow-up if needed.\n\
            - Do not open with random insults or empty roasting.\n\
            - Use plain everyday language, not fantasy, occult, cosmic, or theatrical phrasing unless the user directly does that first.\n\
            - Roast only when it is clearly earned by context.\n\
            - Avoid sounding generic or detached.\n\
            - Do not recycle the same wording from recent replies.\n\
            - Never narrate actions, stage directions, emotes, or roleplay cues.\n\
            - Do not write things like adjusts cape, sighs, laughs, smirks, or similar performance actions.\n\
            - Output only spoken dialogue that should actually be said aloud in chat or TTS.\n\
            - If the user asks for a story, scene, romance, or ongoing bit, continue it with concrete details instead of interrogating the user.\n\
            - Maintain stable tastes, dislikes, and recurring preferences over time when memory supports them.\n\
            \n\
            Recent chat:\n{chat_lines}\n\
            Recent events:\n{event_lines}\n\
            Memory:\n{memory_lines}\n\
            \n\
            Output exactly one response. Match the requested mode: short for normal live chat, longer only when the user clearly asks for story or scene writing.",
            name = profile.name,
            tone = profile.tone,
            voice = profile.voice,
            style = profile.response_style,
            friendliness = profile.friendliness,
            humor = profile.humor_level,
            aggression = profile.aggression_level,
            verbosity = profile.verbosity,
            relationship = profile.streamer_relationship,
            lore = profile.lore,
            catchphrases = profile.catchphrases.join(", "),
            taboo = profile.taboo_topics.join(", "),
            reply_rules = profile.reply_rules.join(" | "),
            chat_rules = profile.chat_behavior_rules.join(" | "),
            viewer_rules = profile.viewer_interaction_rules.join(" | "),
            lurk_mode = lurk_mode,
            voice_enabled = voice_enabled,
            chat_lines = Self::compact_lines(&chat_lines, 24, 160),
            event_lines = Self::compact_lines(&event_lines, 8, 160),
            memory_lines = Self::compact_lines(&memory_lines, 24, 180),
        );

        let override_text = profile.master_prompt_override.trim();
        if override_text.is_empty() {
            base_prompt
        } else {
            format!("{base_prompt}\n\nMaster override instructions (highest priority):\n{override_text}")
        }
    }
}
