use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration;

/// A single GraphQL error from the response.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GraphQLError {
    pub message: String,
    #[serde(default)]
    pub locations: Vec<serde_json::Value>,
    #[serde(default)]
    pub extensions: serde_json::Value,
}

/// The full GraphQL response envelope.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct GraphQLResponse {
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub errors: Vec<GraphQLError>,
}

/// Extracted field suggestion from an error message.
#[derive(Debug, Clone, Serialize)]
pub struct FieldSuggestion {
    /// The invalid field name we queried
    pub queried_field: String,
    /// Suggested valid field names from the error
    pub suggestions: Vec<String>,
    /// The parent type context (if extractable)
    pub parent_type: Option<String>,
}

/// Information extracted from "must have a selection of subfields" errors.
#[derive(Debug, Clone)]
pub struct ObjectTypeHint {
    /// The field name that requires subfields
    pub field_name: String,
    /// The type it returns
    pub type_name: String,
}

// Patterns for extracting suggestions from GraphQL error messages.
// Common formats:
//   - "Cannot query field \"xyz\" on type \"Query\". Did you mean \"abc\" or \"def\"?"
//   - "Cannot query field \"xyz\" on type \"Query\". Did you mean \"abc\", \"def\", or \"ghi\"?"
//   - "Unknown field \"xyz\" on type \"Query\". Did you mean \"abc\"?"  (async-graphql)
static SUGGESTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:Cannot query|Unknown) field "([^"]+)" on type "([^"]+)".*?Did you mean ([^?]+)\?"#,
    )
    .unwrap()
});

static FIELD_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());

// Pattern for "Field X of type Y must have a selection of subfields"
// async-graphql: `Field "user" of type "User" must have a selection of subfields`
static SUBFIELD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[Ff]ield "([^"]+)" of type "([^"]+)".*must have a selection of subfields"#)
        .unwrap()
});

pub struct GraphQLClient {
    client: Client,
    pub endpoint: String,
    delay: Duration,
    auth: Option<String>,
}

impl GraphQLClient {
    pub fn new(endpoint: &str, user_agent: &str, delay_ms: u64, auth: Option<String>) -> Self {
        let client = Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            endpoint: endpoint.to_string(),
            delay: Duration::from_millis(delay_ms),
            auth,
        }
    }

    /// Send a GraphQL query and return the parsed response.
    pub async fn query(&self, query: &str) -> Result<GraphQLResponse, String> {
        // Rate limiting delay
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }

        let body = serde_json::json!({
            "query": query,
        });

        let mut req = self.client.post(&self.endpoint).json(&body);

        if let Some(ref auth) = self.auth {
            req = req.header("Authorization", auth);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        let response = serde_json::from_str::<GraphQLResponse>(&text).map_err(|e| {
            format!(
                "Failed to parse response JSON: {} (body: {})",
                e,
                &text[..text.len().min(200)]
            )
        })?;

        if std::env::var("INTROSPECTME_DEBUG").is_ok() {
            for err in &response.errors {
                eprintln!("[DEBUG] Error: {}", err.message);
            }
        }

        Ok(response)
    }

    /// Send a raw query string and extract field suggestions from error responses.
    pub async fn send_probe(&self, query: &str) -> Result<ProbeResult, String> {
        let response = self.query(query).await?;
        Ok(parse_probe_response(&response.errors))
    }

    /// Probe a field on the root query type.
    pub async fn probe_root_field(&self, probe_field: &str) -> Result<ProbeResult, String> {
        let query = format!("{{ {} }}", probe_field);
        self.send_probe(&query).await
    }

}

/// Result of a probe query, containing all extracted information.
#[derive(Debug, Clone, Default)]
pub struct ProbeResult {
    pub suggestions: Vec<FieldSuggestion>,
    pub object_type_hints: Vec<ObjectTypeHint>,
}

/// Parse all useful information from GraphQL error messages.
fn parse_probe_response(errors: &[GraphQLError]) -> ProbeResult {
    let mut result = ProbeResult::default();

    for error in errors {
        // Check for field suggestions
        if let Some(caps) = SUGGESTION_RE.captures(&error.message) {
            let _field_name = caps.get(1).map(|m| m.as_str().to_string());
            let parent_type = caps.get(2).map(|m| m.as_str().to_string());
            let suggestions_str = caps.get(3).map(|m| m.as_str()).unwrap_or("");

            let suggestions: Vec<String> = FIELD_NAME_RE
                .captures_iter(suggestions_str)
                .map(|c| c.get(1).unwrap().as_str().to_string())
                .filter(|s| !s.starts_with("__")) // Filter introspection fields
                .collect();

            if !suggestions.is_empty() {
                let queried = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                result.suggestions.push(FieldSuggestion {
                    queried_field: queried,
                    suggestions,
                    parent_type,
                });
            }
        }

        // Check for "must have a selection of subfields" hints
        if let Some(caps) = SUBFIELD_RE.captures(&error.message) {
            let field_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let type_name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            result.object_type_hints.push(ObjectTypeHint {
                field_name,
                type_name,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_suggestions_graphql_js() {
        let errors = vec![GraphQLError {
            message:
                r#"Cannot query field "xuser" on type "Query". Did you mean "user" or "users"?"#
                    .to_string(),
            locations: vec![],
            extensions: serde_json::Value::Null,
        }];
        let result = parse_probe_response(&errors);
        assert_eq!(result.suggestions.len(), 1);
        assert_eq!(result.suggestions[0].suggestions, vec!["user", "users"]);
        assert_eq!(
            result.suggestions[0].parent_type,
            Some("Query".to_string())
        );
    }

    #[test]
    fn test_extract_suggestions_async_graphql() {
        let errors = vec![GraphQLError {
            message: r#"Unknown field "xuser" on type "QueryRoot". Did you mean "user"?"#
                .to_string(),
            locations: vec![],
            extensions: serde_json::Value::Null,
        }];
        let result = parse_probe_response(&errors);
        assert_eq!(result.suggestions.len(), 1);
        assert_eq!(result.suggestions[0].suggestions, vec!["user"]);
        assert_eq!(
            result.suggestions[0].parent_type,
            Some("QueryRoot".to_string())
        );
    }

    #[test]
    fn test_extract_suggestions_multiple() {
        let errors = vec![GraphQLError {
            message:
                r#"Cannot query field "xname" on type "User". Did you mean "name", "email", or "role"?"#
                    .to_string(),
            locations: vec![],
            extensions: serde_json::Value::Null,
        }];
        let result = parse_probe_response(&errors);
        assert_eq!(result.suggestions.len(), 1);
        assert_eq!(
            result.suggestions[0].suggestions,
            vec!["name", "email", "role"]
        );
    }

    #[test]
    fn test_extract_subfield_hint() {
        let errors = vec![GraphQLError {
            message: r#"Field "user" of type "User" must have a selection of subfields"#
                .to_string(),
            locations: vec![],
            extensions: serde_json::Value::Null,
        }];
        let result = parse_probe_response(&errors);
        assert_eq!(result.object_type_hints.len(), 1);
        assert_eq!(result.object_type_hints[0].field_name, "user");
        assert_eq!(result.object_type_hints[0].type_name, "User");
    }

    #[test]
    fn test_filters_introspection_fields() {
        let errors = vec![GraphQLError {
            message:
                r#"Unknown field "xtype" on type "QueryRoot". Did you mean "__type" or "user"?"#
                    .to_string(),
            locations: vec![],
            extensions: serde_json::Value::Null,
        }];
        let result = parse_probe_response(&errors);
        assert_eq!(result.suggestions[0].suggestions, vec!["user"]);
    }
}
