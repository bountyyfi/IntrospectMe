#![cfg(feature = "poc")]

use actix_web::{guard, web, App, HttpResponse, HttpServer};
use async_graphql::{
    http::GraphiQLSource, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject, ID,
};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse};
use tokio::sync::oneshot;

// ─── Schema Types ───────────────────────────────────────────────────

#[derive(SimpleObject, Clone)]
pub struct User {
    pub id: ID,
    pub name: String,
    pub email: String,
    pub role: String,
    pub active: bool,
    pub profile: Profile,
    pub orders: Vec<Order>,
}

#[derive(SimpleObject, Clone)]
pub struct Profile {
    pub bio: String,
    pub avatar: String,
    pub website: String,
}

#[derive(SimpleObject, Clone)]
pub struct Order {
    pub id: ID,
    pub total: f64,
    pub status: String,
    pub items: Vec<OrderItem>,
}

#[derive(SimpleObject, Clone)]
pub struct OrderItem {
    pub id: ID,
    pub product: Product,
    pub quantity: i32,
    pub price: f64,
}

#[derive(SimpleObject, Clone)]
pub struct Product {
    pub id: ID,
    pub name: String,
    pub description: String,
    pub price: f64,
    pub category: String,
    pub sku: String,
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn user(&self, id: ID) -> User {
        User {
            id,
            name: "Alice".into(),
            email: "alice@example.com".into(),
            role: "admin".into(),
            active: true,
            profile: Profile {
                bio: "Engineer".into(),
                avatar: "https://example.com/avatar.png".into(),
                website: "https://example.com".into(),
            },
            orders: vec![],
        }
    }

    async fn users(&self) -> Vec<User> {
        vec![]
    }

    async fn me(&self) -> User {
        User {
            id: "1".into(),
            name: "Current User".into(),
            email: "me@example.com".into(),
            role: "user".into(),
            active: true,
            profile: Profile {
                bio: "".into(),
                avatar: "".into(),
                website: "".into(),
            },
            orders: vec![],
        }
    }

    async fn product(&self, id: ID) -> Product {
        Product {
            id,
            name: "Widget".into(),
            description: "A widget".into(),
            price: 9.99,
            category: "gadgets".into(),
            sku: "WDG-001".into(),
        }
    }

    async fn products(&self) -> Vec<Product> {
        vec![]
    }

    async fn order(&self, id: ID) -> Order {
        Order {
            id,
            total: 29.99,
            status: "pending".into(),
            items: vec![],
        }
    }

    async fn orders(&self) -> Vec<Order> {
        vec![]
    }

    async fn search(&self, query: String) -> Vec<Product> {
        let _ = query;
        vec![]
    }
}

type PocSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

async fn graphql_handler(schema: web::Data<PocSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

async fn graphiql() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(GraphiQLSource::build().endpoint("/graphql").finish())
}

/// Start the PoC GraphQL server and return its URL + a shutdown handle.
pub async fn start_poc_server() -> Result<(String, oneshot::Sender<()>), String> {
    // Build schema with introspection disabled
    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .disable_introspection()
        .finish();

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let port = find_available_port().await?;
    let addr = format!("127.0.0.1:{}", port);
    let url = format!("http://{}/graphql", addr);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(schema.clone()))
            .service(
                web::resource("/graphql")
                    .guard(guard::Post())
                    .to(graphql_handler),
            )
            .service(
                web::resource("/graphql")
                    .guard(guard::Get())
                    .to(graphiql),
            )
    })
    .bind(&addr)
    .map_err(|e| format!("Failed to bind server: {}", e))?
    .workers(2)
    .run();

    let server_handle = server.handle();

    // Spawn the server
    tokio::spawn(async move {
        let _ = server.await;
    });

    // Spawn shutdown listener
    tokio::spawn(async move {
        let _ = shutdown_rx.await;
        server_handle.stop(true).await;
    });

    // Wait a moment for server to start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    Ok((url, shutdown_tx))
}

async fn find_available_port() -> Result<u16, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to find available port: {}", e))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local addr: {}", e))?
        .port();
    drop(listener);
    Ok(port)
}

/// Generate the real SDL for the PoC schema (for comparison).
pub fn real_schema_sdl() -> String {
    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .finish();
    schema.sdl()
}

/// Print a side-by-side comparison of real vs reconstructed schema.
pub fn print_comparison(real_sdl: &str, reconstructed_sdl: &str) {
    println!("\n{}", "=".repeat(80));
    println!("SCHEMA COMPARISON");
    println!("{}\n", "=".repeat(80));

    println!("--- REAL SCHEMA (from introspection) ---");
    println!("{}", "-".repeat(40));
    println!("{}\n", real_sdl);

    println!("--- RECONSTRUCTED SCHEMA (from suggestions) ---");
    println!("{}", "-".repeat(40));
    println!("{}\n", reconstructed_sdl);

    // Count types and fields in both
    let real_types = count_types(real_sdl);
    let recon_types = count_types(reconstructed_sdl);
    let real_fields = count_fields(real_sdl);
    let recon_fields = count_fields(reconstructed_sdl);

    println!("{}", "=".repeat(80));
    println!("STATISTICS");
    println!("{}", "=".repeat(80));
    println!("                Real    Reconstructed");
    println!("  Types:        {:>4}    {:>4}", real_types, recon_types);
    println!("  Fields:       {:>4}    {:>4}", real_fields, recon_fields);
    println!("{}", "=".repeat(80));
}

fn count_types(sdl: &str) -> usize {
    sdl.lines().filter(|l| l.starts_with("type ")).count()
}

fn count_fields(sdl: &str) -> usize {
    sdl.lines()
        .filter(|l| {
            let trimmed = l.trim();
            trimmed.contains(':')
                && !trimmed.starts_with("type ")
                && !trimmed.starts_with("schema")
                && !trimmed.starts_with("query:")
                && !trimmed.starts_with("mutation:")
                && !trimmed.starts_with('#')
        })
        .count()
}
