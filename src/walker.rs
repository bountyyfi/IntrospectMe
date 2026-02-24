use crate::client::GraphQLClient;
use crate::schema::ReconstructedSchema;
use crate::wordlist;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

/// The recursive type walker that discovers the schema by probing fields.
pub struct TypeWalker {
    client: Arc<GraphQLClient>,
    schema: Arc<Mutex<ReconstructedSchema>>,
    probed_types: Arc<Mutex<HashSet<String>>>,
    max_depth: usize,
    progress: ProgressBar,
}

impl TypeWalker {
    pub fn new(
        client: Arc<GraphQLClient>,
        schema: Arc<Mutex<ReconstructedSchema>>,
        max_depth: usize,
    ) -> Self {
        let progress = ProgressBar::new_spinner();
        progress.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );

        Self {
            client,
            schema,
            probed_types: Arc::new(Mutex::new(HashSet::new())),
            max_depth,
            progress,
        }
    }

    pub async fn run(&self) -> Result<(), String> {
        self.progress
            .set_message("Starting schema reconstruction...");

        // Phase 1: Discover root Query fields
        self.progress
            .set_message("Phase 1: Probing root Query type...");
        let (root_type_name, object_fields) = self.probe_root_type().await?;
        self.schema.lock().await.query_type = root_type_name.clone();

        // Phase 2: Recursively probe nested types
        self.progress
            .set_message("Phase 2: Probing nested types...");

        for (field_name, type_name) in &object_fields {
            self.schema
                .lock()
                .await
                .set_field_type(&root_type_name, field_name, type_name);

            // Determine best context queries for this field
            let contexts = build_root_context_queries(field_name);
            self.probe_type_recursive(type_name, &contexts, 1).await?;
        }

        let schema = self.schema.lock().await;
        self.progress.finish_with_message(format!(
            "Discovery complete! Found {} types, {} fields",
            schema.types.len(),
            schema
                .types
                .values()
                .map(|t| t.fields.len())
                .sum::<usize>()
        ));

        Ok(())
    }

    /// Probe the root Query type.
    /// Returns (root_type_name, Vec<(field_name, return_type_name)>).
    async fn probe_root_type(&self) -> Result<(String, Vec<(String, String)>), String> {
        let probes = wordlist::full_probe_list();
        let mut discovered_fields: HashSet<String> = HashSet::new();
        let mut root_type_name = "Query".to_string();
        let mut object_fields: HashMap<String, String> = HashMap::new();

        let total = probes.len();
        for (i, probe) in probes.iter().enumerate() {
            self.progress.set_message(format!(
                "Probing root: {} [{}/{}]",
                probe,
                i + 1,
                total
            ));

            match self.client.probe_root_field(&probe).await {
                Ok(result) => {
                    for suggestion in &result.suggestions {
                        let parent = suggestion
                            .parent_type
                            .clone()
                            .unwrap_or_else(|| "Query".to_string());

                        if root_type_name == "Query" && parent != "Query" {
                            root_type_name = parent.clone();
                        }

                        self.schema.lock().await.log_discovery(
                            &parent,
                            &suggestion.queried_field,
                            &suggestion.suggestions,
                        );

                        for field_name in &suggestion.suggestions {
                            let mut schema = self.schema.lock().await;
                            if schema.add_field(&parent, field_name) {
                                discovered_fields.insert(field_name.clone());
                                self.progress
                                    .println(format!("  [+] Found: {}.{}", parent, field_name));
                            }
                        }
                    }

                    for hint in &result.object_type_hints {
                        object_fields
                            .entry(hint.field_name.clone())
                            .or_insert_with(|| hint.type_name.clone());
                        self.progress.println(format!(
                            "  [>] Type hint: root.{} -> {}",
                            hint.field_name, hint.type_name
                        ));
                    }
                }
                Err(e) => {
                    if std::env::var("INTROSPECTME_DEBUG").is_ok() {
                        self.progress
                            .println(format!("  [!] Root probe error: {}", e));
                    }
                }
            }
            self.progress.tick();
        }

        // For fields not yet identified as object types, send bare queries to check
        let fields_to_check: Vec<String> = discovered_fields
            .iter()
            .filter(|f| !is_likely_scalar(f) && !object_fields.contains_key(*f))
            .cloned()
            .collect();

        for field_name in &fields_to_check {
            self.progress
                .set_message(format!("Checking type of {}...", field_name));
            let query = format!("{{ {} }}", field_name);
            if let Ok(result) = self.client.send_probe(&query).await {
                for hint in &result.object_type_hints {
                    object_fields
                        .entry(hint.field_name.clone())
                        .or_insert_with(|| hint.type_name.clone());
                    self.progress.println(format!(
                        "  [>] Type hint: root.{} -> {}",
                        hint.field_name, hint.type_name
                    ));
                }
            }
        }

        Ok((root_type_name, object_fields.into_iter().collect()))
    }

    /// Recursively probe a type using the given context queries to reach it.
    /// `contexts` is a list of query prefixes that can reach this type.
    /// E.g., for User: ["users", "user(id: \"1\")"]
    /// For Profile (through User): ["users { profile", "user(id: \"1\") { profile"]
    async fn probe_type_recursive(
        &self,
        type_name: &str,
        contexts: &[String],
        depth: usize,
    ) -> Result<(), String> {
        if depth > self.max_depth {
            return Ok(());
        }

        {
            let mut probed = self.probed_types.lock().await;
            if !probed.insert(type_name.to_string()) {
                return Ok(());
            }
        }

        self.progress.println(format!(
            "  [*] Probing type: {} (depth {})",
            type_name, depth
        ));

        let probes = wordlist::full_probe_list();
        let mut discovered_fields: HashSet<String> = HashSet::new();
        let mut child_object_types: HashMap<String, String> = HashMap::new();

        let total = probes.len();
        for (i, probe) in probes.iter().enumerate() {
            self.progress.set_message(format!(
                "Probing {}.{} [{}/{}]",
                type_name,
                probe,
                i + 1,
                total
            ));

            // Try each context query pattern
            let mut found = false;
            for ctx in contexts {
                // Close any open braces in the context with the probe field
                let query = format!("{{ {} {{ {} }} }}", ctx, probe);
                let closing_braces = ctx.matches('{').count();
                let query = format!("{}{}", query, " }".repeat(closing_braces));

                match self.client.send_probe(&query).await {
                    Ok(result) => {
                        for suggestion in &result.suggestions {
                            let parent = suggestion
                                .parent_type
                                .clone()
                                .unwrap_or_else(|| type_name.to_string());

                            if parent != type_name {
                                continue;
                            }

                            self.schema.lock().await.log_discovery(
                                &parent,
                                &suggestion.queried_field,
                                &suggestion.suggestions,
                            );

                            for field_name in &suggestion.suggestions {
                                let mut schema = self.schema.lock().await;
                                if schema.add_field(&parent, field_name) {
                                    discovered_fields.insert(field_name.clone());
                                    self.progress.println(format!(
                                        "  [+] Found: {}.{}",
                                        parent, field_name
                                    ));
                                }
                            }
                            found = true;
                        }

                        for hint in &result.object_type_hints {
                            if hint.type_name != type_name {
                                child_object_types
                                    .entry(hint.field_name.clone())
                                    .or_insert_with(|| hint.type_name.clone());
                            }
                        }

                        if found {
                            break;
                        }
                    }
                    Err(_) => {}
                }
            }
            self.progress.tick();
        }

        // Check discovered fields for object types
        let fields_to_check: Vec<String> = discovered_fields
            .iter()
            .filter(|f| !is_likely_scalar(f) && !child_object_types.contains_key(*f))
            .cloned()
            .collect();

        for field_name in &fields_to_check {
            for ctx in contexts {
                let query = format!("{{ {} {{ {} }} }}", ctx, field_name);
                let closing_braces = ctx.matches('{').count();
                let query = format!("{}{}", query, " }".repeat(closing_braces));

                if let Ok(result) = self.client.send_probe(&query).await {
                    for hint in &result.object_type_hints {
                        if hint.field_name == *field_name {
                            child_object_types
                                .entry(hint.field_name.clone())
                                .or_insert_with(|| hint.type_name.clone());
                            self.progress.println(format!(
                                "  [>] Type hint: {}.{} -> {}",
                                type_name, hint.field_name, hint.type_name
                            ));
                        }
                    }
                    if child_object_types.contains_key(field_name) {
                        break;
                    }
                }
            }
        }

        // Recurse into child types
        for (field_name, child_type) in &child_object_types {
            self.schema
                .lock()
                .await
                .set_field_type(type_name, field_name, child_type);

            // Build context queries for the child type by extending each parent context
            let child_contexts: Vec<String> = contexts
                .iter()
                .map(|ctx| format!("{} {{ {}", ctx, field_name))
                .collect();

            Box::pin(self.probe_type_recursive(child_type, &child_contexts, depth + 1))
                .await?;
        }

        Ok(())
    }
}

/// Build context queries for reaching a type from a root field.
/// Tries both bare field and field-with-id-arg patterns.
fn build_root_context_queries(field_name: &str) -> Vec<String> {
    vec![
        field_name.to_string(),
        format!("{}(id: \"1\")", field_name),
    ]
}

/// Heuristic: check if a field name is likely a scalar (not an object type).
fn is_likely_scalar(field_name: &str) -> bool {
    let scalars = [
        "id",
        "name",
        "email",
        "password",
        "token",
        "title",
        "description",
        "body",
        "text",
        "content",
        "message",
        "slug",
        "url",
        "phone",
        "code",
        "key",
        "value",
        "label",
        "bio",
        "website",
        "username",
        "displayName",
        "firstName",
        "lastName",
        "avatar",
        "image",
        "status",
        "state",
        "type",
        "role",
        "active",
        "enabled",
        "deleted",
        "success",
        "price",
        "amount",
        "total",
        "subtotal",
        "tax",
        "discount",
        "quantity",
        "sku",
        "zip",
        "city",
        "country",
        "date",
        "timestamp",
        "createdAt",
        "updatedAt",
        "deletedAt",
        "totalCount",
        "hasNextPage",
        "hasPreviousPage",
        "startCursor",
        "endCursor",
        "cursor",
        "limit",
        "offset",
        "pageSize",
        "sort",
        "sortBy",
        "category",
    ];

    scalars.contains(&field_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_likely_scalar() {
        assert!(is_likely_scalar("id"));
        assert!(is_likely_scalar("name"));
        assert!(is_likely_scalar("email"));
        assert!(is_likely_scalar("createdAt"));
        assert!(!is_likely_scalar("user"));
        assert!(!is_likely_scalar("orders"));
        assert!(!is_likely_scalar("profile"));
    }

    #[test]
    fn test_build_root_context_queries() {
        let contexts = build_root_context_queries("user");
        assert_eq!(contexts, vec!["user", "user(id: \"1\")"]);
    }
}
