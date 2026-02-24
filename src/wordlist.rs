/// Built-in wordlist of common GraphQL field names.
pub const BASE_WORDS: &[&str] = &[
    // Identity / Auth
    "user",
    "users",
    "me",
    "account",
    "accounts",
    "admin",
    "admins",
    "email",
    "password",
    "token",
    "tokens",
    "login",
    "logout",
    "register",
    "signup",
    "session",
    "sessions",
    "auth",
    "authenticate",
    "authorization",
    "permission",
    "permissions",
    "role",
    "roles",
    // Common fields
    "id",
    "name",
    "title",
    "description",
    "type",
    "status",
    "state",
    "enabled",
    "active",
    "created",
    "createdAt",
    "updated",
    "updatedAt",
    "deleted",
    "deletedAt",
    "date",
    "timestamp",
    "slug",
    "url",
    "image",
    "images",
    "avatar",
    "phone",
    "address",
    "city",
    "country",
    "zip",
    "code",
    "key",
    "value",
    "label",
    "message",
    "content",
    "body",
    "text",
    "comment",
    "comments",
    "tag",
    "tags",
    "category",
    "categories",
    // Profile
    "profile",
    "profiles",
    "firstName",
    "lastName",
    "username",
    "displayName",
    "bio",
    "website",
    // E-commerce
    "product",
    "products",
    "order",
    "orders",
    "cart",
    "carts",
    "checkout",
    "payment",
    "payments",
    "price",
    "amount",
    "total",
    "subtotal",
    "tax",
    "discount",
    "coupon",
    "shipping",
    "inventory",
    "sku",
    "quantity",
    "item",
    "items",
    // Search / Pagination
    "search",
    "filter",
    "filters",
    "sort",
    "sortBy",
    "limit",
    "offset",
    "cursor",
    "after",
    "before",
    "first",
    "last",
    "page",
    "pageSize",
    "skip",
    // Relay-style
    "node",
    "nodes",
    "edge",
    "edges",
    "connection",
    "connections",
    "pageInfo",
    "totalCount",
    "hasNextPage",
    "hasPreviousPage",
    "startCursor",
    "endCursor",
    // Mutations
    "create",
    "update",
    "delete",
    "remove",
    "add",
    "set",
    "input",
    "data",
    "result",
    "success",
    "error",
    "errors",
    // Subscriptions
    "subscription",
    "subscriptions",
    "event",
    "events",
    "notification",
    "notifications",
    // API / System
    "query",
    "mutation",
    "schema",
    "health",
    "version",
    "config",
    "configuration",
    "settings",
    "setting",
    "log",
    "logs",
    "audit",
    "analytics",
    "metrics",
    "stats",
    "statistics",
    "report",
    "reports",
    "dashboard",
    // Relationships
    "parent",
    "children",
    "child",
    "group",
    "groups",
    "team",
    "teams",
    "member",
    "members",
    "organization",
    "organizations",
    "org",
    "workspace",
    "project",
    "projects",
    "repository",
    "repositories",
    "file",
    "files",
    "folder",
    "folders",
    "document",
    "documents",
    "post",
    "posts",
    "article",
    "articles",
    "thread",
    "threads",
    "channel",
    "channels",
    "viewer",
];

/// Generate typo mutations of a word to maximize "Did you mean...?" hits.
pub fn generate_mutations(word: &str) -> Vec<String> {
    let mut mutations = Vec::new();

    // Original word with a typo prefix -- almost guaranteed to not match
    // but close enough to trigger suggestions
    mutations.push(format!("x{}", word));

    // Drop last character
    if word.len() > 2 {
        mutations.push(word[..word.len() - 1].to_string());
    }

    // Swap first two chars
    if word.len() >= 2 {
        let chars: Vec<char> = word.chars().collect();
        let mut swapped = chars.clone();
        swapped.swap(0, 1);
        let s: String = swapped.into_iter().collect();
        if s != word {
            mutations.push(s);
        }
    }

    // Add common suffixes
    mutations.push(format!("{}s", word));
    mutations.push(format!("{}Id", word));
    mutations.push(format!("{}By", word));
    mutations.push(format!("{}List", word));

    // Uppercase first letter variant
    if let Some(first) = word.chars().next() {
        if first.is_lowercase() {
            let upper: String = first.to_uppercase().collect::<String>() + &word[first.len_utf8()..];
            mutations.push(upper);
        }
    }

    // Lowercase first letter variant
    if let Some(first) = word.chars().next() {
        if first.is_uppercase() {
            let lower: String = first.to_lowercase().collect::<String>() + &word[first.len_utf8()..];
            mutations.push(lower);
        }
    }

    mutations
}

/// Get the full probe wordlist: base words + all mutations, deduplicated.
pub fn full_probe_list() -> Vec<String> {
    let mut probes: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for &word in BASE_WORDS {
        // Add mutations of each base word
        for mutation in generate_mutations(word) {
            if seen.insert(mutation.clone()) {
                probes.push(mutation);
            }
        }
    }

    probes
}
