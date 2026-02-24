use serde::Serialize;
use std::collections::BTreeMap;

/// Represents a discovered GraphQL type with its fields.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredType {
    pub name: String,
    pub fields: BTreeMap<String, FieldInfo>,
}

/// Information about a discovered field.
#[derive(Debug, Clone, Serialize)]
pub struct FieldInfo {
    pub name: String,
    /// The return type name if we were able to discover it (by probing subfields).
    /// If we found subfields, this is the discovered type name; otherwise None (scalar).
    pub type_name: Option<String>,
    /// Whether this field appears to be a list (heuristic based on name patterns).
    pub is_list: bool,
}

/// The fully reconstructed schema from probing.
#[derive(Debug, Clone, Serialize)]
pub struct ReconstructedSchema {
    pub types: BTreeMap<String, DiscoveredType>,
    pub query_type: String,
    /// Raw discovery log: all suggestions we received.
    pub discovery_log: Vec<DiscoveryEntry>,
}

/// A single discovery entry for the JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryEntry {
    pub parent_type: String,
    pub probed_field: String,
    pub discovered_fields: Vec<String>,
}

impl ReconstructedSchema {
    pub fn new() -> Self {
        Self {
            types: BTreeMap::new(),
            query_type: "Query".to_string(),
            discovery_log: Vec::new(),
        }
    }

    /// Register a discovered field on a type. Returns true if the field was new.
    pub fn add_field(&mut self, type_name: &str, field_name: &str) -> bool {
        let typ = self
            .types
            .entry(type_name.to_string())
            .or_insert_with(|| DiscoveredType {
                name: type_name.to_string(),
                fields: BTreeMap::new(),
            });

        if typ.fields.contains_key(field_name) {
            return false;
        }

        let is_list = field_name.ends_with('s')
            && !field_name.ends_with("ss")
            && field_name != "address"
            && field_name != "status"
            && field_name != "success";

        typ.fields.insert(
            field_name.to_string(),
            FieldInfo {
                name: field_name.to_string(),
                type_name: None,
                is_list,
            },
        );

        true
    }

    /// Set the return type of a field.
    pub fn set_field_type(&mut self, parent_type: &str, field_name: &str, type_name: &str) {
        if let Some(typ) = self.types.get_mut(parent_type) {
            if let Some(field) = typ.fields.get_mut(field_name) {
                field.type_name = Some(type_name.to_string());
            }
        }
    }

    /// Log a discovery for JSON output.
    pub fn log_discovery(
        &mut self,
        parent_type: &str,
        probed_field: &str,
        discovered: &[String],
    ) {
        self.discovery_log.push(DiscoveryEntry {
            parent_type: parent_type.to_string(),
            probed_field: probed_field.to_string(),
            discovered_fields: discovered.to_vec(),
        });
    }

    /// Generate SDL (Schema Definition Language) output.
    pub fn to_sdl(&self) -> String {
        let mut sdl = String::new();

        // Schema definition
        sdl.push_str("schema {\n");
        sdl.push_str(&format!("  query: {}\n", self.query_type));
        sdl.push_str("}\n\n");

        // Sort types so Query comes first, then alphabetical
        let mut type_names: Vec<&String> = self.types.keys().collect();
        type_names.sort_by(|a, b| {
            if a.as_str() == self.query_type {
                std::cmp::Ordering::Less
            } else if b.as_str() == self.query_type {
                std::cmp::Ordering::Greater
            } else {
                a.cmp(b)
            }
        });

        for type_name in type_names {
            let typ = &self.types[type_name];
            sdl.push_str(&format!("type {} {{\n", type_name));

            for field in typ.fields.values() {
                let type_str = match &field.type_name {
                    Some(t) => {
                        if field.is_list {
                            format!("[{}]", t)
                        } else {
                            t.clone()
                        }
                    }
                    None => {
                        // Infer scalar type from field name heuristics
                        infer_scalar_type(&field.name)
                    }
                };
                sdl.push_str(&format!("  {}: {}\n", field.name, type_str));
            }

            sdl.push_str("}\n\n");
        }

        sdl.trim_end().to_string()
    }

}

/// Infer a scalar type from a field name using common naming conventions.
fn infer_scalar_type(field_name: &str) -> String {
    let lower = field_name.to_lowercase();

    if lower == "id" || lower.ends_with("_id") || lower.ends_with("id") {
        return "ID".to_string();
    }

    if lower.contains("count")
        || lower == "limit"
        || lower == "offset"
        || lower == "quantity"
        || lower == "total"
        || lower == "amount"
        || lower == "price"
        || lower == "age"
        || lower == "size"
    {
        return "Int".to_string();
    }

    if lower.contains("is_")
        || lower.starts_with("is")
        || lower.starts_with("has")
        || lower == "active"
        || lower == "enabled"
        || lower == "deleted"
        || lower == "success"
    {
        return "Boolean".to_string();
    }

    if lower.contains("at")
        && (lower.contains("created")
            || lower.contains("updated")
            || lower.contains("deleted"))
    {
        return "DateTime".to_string();
    }

    if lower == "date" || lower == "timestamp" {
        return "DateTime".to_string();
    }

    "String".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_field() {
        let mut schema = ReconstructedSchema::new();
        assert!(schema.add_field("Query", "user"));
        assert!(!schema.add_field("Query", "user")); // duplicate
        assert!(schema.add_field("Query", "users"));
    }

    #[test]
    fn test_sdl_output() {
        let mut schema = ReconstructedSchema::new();
        schema.add_field("Query", "user");
        schema.set_field_type("Query", "user", "User");
        schema.add_field("Query", "users");
        schema.set_field_type("Query", "users", "User");
        schema.add_field("User", "id");
        schema.add_field("User", "name");
        schema.add_field("User", "email");

        let sdl = schema.to_sdl();
        assert!(sdl.contains("type Query {"));
        assert!(sdl.contains("user: User"));
        assert!(sdl.contains("users: [User]"));
        assert!(sdl.contains("type User {"));
        assert!(sdl.contains("id: ID"));
        assert!(sdl.contains("name: String"));
    }

    #[test]
    fn test_infer_scalar_types() {
        assert_eq!(infer_scalar_type("id"), "ID");
        assert_eq!(infer_scalar_type("userId"), "ID");
        assert_eq!(infer_scalar_type("name"), "String");
        assert_eq!(infer_scalar_type("email"), "String");
        assert_eq!(infer_scalar_type("totalCount"), "Int");
        assert_eq!(infer_scalar_type("isActive"), "Boolean");
        assert_eq!(infer_scalar_type("createdAt"), "DateTime");
    }
}
