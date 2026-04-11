use serde::Deserialize;

use crate::{config::SearchConfig, error::{AppError, AppResult}};

#[derive(Debug, Deserialize)]
struct DuckResponse {
    #[serde(rename = "AbstractText")]
    abstract_text: String,
    #[serde(rename = "AbstractURL")]
    abstract_url: String,
    #[serde(rename = "RelatedTopics")]
    related_topics: Vec<RelatedTopic>,
}

#[derive(Debug, Deserialize)]
struct RelatedTopic {
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "FirstURL")]
    first_url: Option<String>,
}

#[derive(Clone)]
pub struct SearchService {
    client: reqwest::Client,
}

impl SearchService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn search(&self, config: &SearchConfig, query: &str) -> AppResult<String> {
        if !config.enabled {
            return Err(AppError::Search("search is disabled in config".to_string()));
        }

        let endpoint = "https://api.duckduckgo.com/";
        let response = self
            .client
            .get(endpoint)
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Search(format!(
                "search provider returned {}",
                response.status()
            )));
        }

        let body: DuckResponse = response.json().await?;
        if !body.abstract_text.trim().is_empty() {
            return Ok(format!("{} (source: {})", body.abstract_text, body.abstract_url));
        }

        let picks = body
            .related_topics
            .into_iter()
            .filter_map(|x| match (x.text, x.first_url) {
                (Some(text), Some(url)) => Some(format!("- {} ({})", text, url)),
                _ => None,
            })
            .take(config.max_results)
            .collect::<Vec<_>>();

        if picks.is_empty() {
            return Ok("No concise web result found for that query.".to_string());
        }

        Ok(format!("Search summary:\n{}", picks.join("\n")))
    }
}
