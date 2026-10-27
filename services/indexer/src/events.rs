use serde::Deserialize;
use serde_json::Value;

/// A NEP-297 event: `EVENT_JSON:{"standard","version","event","data":[...]}`.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Event {
    pub standard: String,
    pub version: String,
    pub event: String,
    #[serde(default)]
    pub data: Vec<Value>,
}

pub fn parse(log: &str) -> Option<Event> {
    let json = log.strip_prefix("EVENT_JSON:")?;
    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nep297() {
        let e = parse(r#"EVENT_JSON:{"standard":"clawdao","version":"1.0.0","event":"receipt","data":[{"id":0}]}"#).unwrap();
        assert_eq!(e.event, "receipt");
        assert_eq!(e.data[0]["id"], 0);
    }

    #[test]
    fn ignores_plain_logs() {
        assert!(parse("No voting power at snapshot").is_none());
        assert!(parse("EVENT_JSON:not json").is_none());
    }
}
