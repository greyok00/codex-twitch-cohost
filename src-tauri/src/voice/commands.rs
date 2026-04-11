#[derive(Debug, Clone)]
pub enum VoiceCommand {
    Search(String),
    Open(String),
    Reply(String),
    SwitchModel(String),
    ToggleLurk,
    ToggleTts,
    Summarize,
    Unknown,
}

pub fn parse_voice_command(input: &str) -> VoiceCommand {
    let clean = input.trim();
    let lower = clean.to_lowercase();

    if let Some(rest) = lower.strip_prefix("search for ") {
        return VoiceCommand::Search(rest.trim().to_string());
    }
    if let Some(rest) = clean.strip_prefix("open ") {
        return VoiceCommand::Open(rest.trim().to_string());
    }
    if let Some(rest) = clean.strip_prefix("reply to chat ") {
        return VoiceCommand::Reply(rest.trim().to_string());
    }
    if let Some(rest) = clean.strip_prefix("switch to model ") {
        return VoiceCommand::SwitchModel(rest.trim().to_string());
    }
    if lower.contains("toggle lurk mode") {
        return VoiceCommand::ToggleLurk;
    }
    if lower.contains("toggle tts") || lower.contains("read that aloud") {
        return VoiceCommand::ToggleTts;
    }
    if lower.contains("summarize the last minute") {
        return VoiceCommand::Summarize;
    }

    VoiceCommand::Unknown
}
