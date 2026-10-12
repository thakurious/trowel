/// EmbeddingGemma 2 task prompts. Must match `upstream/Sources/DigUpKit/Model.swift`.
pub struct Prompts;

impl Prompts {
    pub fn query(text: &str) -> String {
        format!("task: search result | query: {text}")
    }

    pub fn code_query(text: &str) -> String {
        format!("task: code retrieval | query: {text}")
    }

    /// `title: none | text: …` when the title is missing or empty.
    /// `upstream/Sources/DigUpKit/Model.swift:97`
    pub fn document(title: Option<&str>, text: &str) -> String {
        let title = match title {
            Some(t) if !t.is_empty() => t,
            _ => "none",
        };
        format!("title: {title} | text: {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::Prompts;

    #[test]
    fn query_matches_upstream() {
        assert_eq!(
            Prompts::query("payment declined error"),
            "task: search result | query: payment declined error"
        );
    }

    #[test]
    fn document_none_title() {
        assert_eq!(
            Prompts::document(None, "hello"),
            "title: none | text: hello"
        );
        assert_eq!(
            Prompts::document(Some(""), "hello"),
            "title: none | text: hello"
        );
    }
}
