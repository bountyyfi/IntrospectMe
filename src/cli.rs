use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "introspectme",
    about = "Reconstruct GraphQL schemas via field suggestion error analysis",
    long_about = "IntrospectMe reconstructs GraphQL schemas by exploiting 'Did you mean X?' \
                  field suggestion errors. No introspection queries are ever sent."
)]
pub struct Cli {
    /// Target GraphQL endpoint URL
    #[arg(long, required_unless_present = "poc")]
    pub url: Option<String>,

    /// Output file path for reconstructed SDL schema
    #[arg(long, default_value = "schema.graphql")]
    pub output: String,

    /// Output file path for raw JSON discovery data
    #[arg(long, default_value = "discovered.json")]
    pub json_output: String,

    /// Delay between requests in milliseconds
    #[arg(long, default_value_t = 100)]
    pub delay: u64,

    /// Custom User-Agent header
    #[arg(long, default_value = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")]
    pub user_agent: String,

    /// Maximum recursion depth for type walking
    #[arg(long, default_value_t = 10)]
    pub depth: usize,

    /// Number of concurrent requests
    #[arg(long, default_value_t = 1)]
    pub concurrency: usize,

    /// Run in PoC mode: spin up a local GraphQL server and demonstrate reconstruction
    #[cfg(feature = "poc")]
    #[arg(long)]
    pub poc: bool,

    /// Custom authorization header value (e.g., "Bearer token123")
    #[arg(long)]
    pub auth: Option<String>,
}
