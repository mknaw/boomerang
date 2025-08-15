use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, trace};

use crate::tools::tool_trait::{Tool, ToolError, ToolOutput};

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchParams {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_depth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_images: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_image_descriptions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_raw_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_favicon: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub content: String,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageResult {
    Simple(String),
    Detailed {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchResponse {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up_questions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<ImageResult>>,
    pub results: Vec<WebSearchResult>,
}

pub struct WebSearchTool {
    client: Client,
    api_key: String,
    base_url: String,
}

impl WebSearchTool {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.tavily.com/search".to_string(),
        }
    }

    pub async fn search(&self, mut params: WebSearchParams) -> Result<WebSearchResponse> {
        debug!("Starting web search with query: '{}'", params.query);
        trace!("Full search parameters: {:?}", params);

        if params.country.is_some() {
            params.topic = Some("general".to_string());
            debug!("Set topic to 'general' due to country parameter");
        }

        let mut request_body = serde_json::to_value(&params)?;
        if let Some(obj) = request_body.as_object_mut() {
            obj.insert(
                "api_key".to_string(),
                serde_json::Value::String(self.api_key.clone()),
            );
        }

        debug!("Making request to {}", self.base_url);
        trace!(
            "Request body: {}",
            serde_json::to_string_pretty(&request_body)?
        );

        let response = self
            .client
            .post(&self.base_url)
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("X-Client-Source", "MCP")
            .json(&request_body)
            .send()
            .await?;

        debug!("Received response with status: {}", response.status());

        if response.status() == 401 {
            debug!("Authentication failed: Invalid API key");
            return Err(anyhow::anyhow!("Invalid API key"));
        } else if response.status() == 429 {
            debug!("Rate limit exceeded");
            return Err(anyhow::anyhow!("Usage limit exceeded"));
        }

        let response_text = response.text().await?;
        trace!("Raw response body: {}", response_text);

        let search_response: WebSearchResponse = serde_json::from_str(&response_text)?;
        debug!(
            "Successfully parsed response with {} results",
            search_response.results.len()
        );

        if let Some(answer) = &search_response.answer {
            debug!("Response includes direct answer: {:.100}...", answer);
        }

        Ok(search_response)
    }

    pub fn format_results(&self, response: &WebSearchResponse) -> String {
        let mut output = Vec::new();

        if let Some(answer) = &response.answer {
            output.push(format!("Answer: {}", answer));
        }

        output.push("Detailed Results:".to_string());

        for result in &response.results {
            output.push(format!("\nTitle: {}", result.title));
            output.push(format!("URL: {}", result.url));
            output.push(format!("Content: {}", result.content));

            if let Some(raw_content) = &result.raw_content {
                output.push(format!("Raw Content: {}", raw_content));
            }

            if let Some(favicon) = &result.favicon {
                output.push(format!("Favicon: {}", favicon));
            }
        }

        if let Some(images) = &response.images
            && !images.is_empty()
        {
            output.push("\nImages:".to_string());
            for (index, image) in images.iter().enumerate() {
                match image {
                    ImageResult::Simple(url) => {
                        output.push(format!("\n[{}] URL: {}", index + 1, url));
                    }
                    ImageResult::Detailed { url, description } => {
                        output.push(format!("\n[{}] URL: {}", index + 1, url));
                        if let Some(desc) = description {
                            output.push(format!("   Description: {}", desc));
                        }
                    }
                }
            }
        }

        output.join("\n")
    }
}

impl Default for WebSearchParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            search_depth: Some("basic".to_string()),
            topic: Some("general".to_string()),
            days: Some(3),
            time_range: None,
            start_date: None,
            end_date: None,
            max_results: Some(10),
            include_images: Some(false),
            include_image_descriptions: Some(false),
            include_raw_content: Some(false),
            include_domains: None,
            exclude_domains: None,
            country: None,
            include_favicon: Some(false),
        }
    }
}

impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for real-time information"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "max_results": {
                    "type": "number",
                    "description": "Maximum number of results to return (5-20)",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, args: serde_json::Value) -> crate::tools::tool_trait::ToolFuture {
        let query = args["query"].as_str().unwrap_or("").to_string();
        let max_results = args["max_results"].as_u64().unwrap_or(10) as u32;
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let tool = WebSearchTool::new(api_key);
            let params = WebSearchParams {
                query,
                max_results: Some(max_results),
                ..Default::default()
            };

            match tool.search(params).await {
                Ok(response) => {
                    let formatted = tool.format_results(&response);
                    Ok(ToolOutput::new(formatted))
                }
                Err(e) => Err(ToolError::new(format!("Search failed: {}", e))),
            }
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }
}
