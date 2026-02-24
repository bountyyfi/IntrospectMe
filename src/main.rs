mod cli;
mod client;
mod schema;
mod walker;
mod wordlist;

#[cfg(feature = "poc")]
mod poc;

use clap::Parser;
use cli::Cli;
use client::GraphQLClient;
use schema::ReconstructedSchema;
use std::sync::Arc;
use tokio::sync::Mutex;
use walker::TypeWalker;

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    #[cfg(feature = "poc")]
    if args.poc {
        run_poc_mode(&args).await;
        return;
    }

    let url = match &args.url {
        Some(url) => url.clone(),
        None => {
            eprintln!("Error: --url is required (or use --poc for demo mode)");
            std::process::exit(1);
        }
    };

    run_reconstruction(&url, &args).await;
}

async fn run_reconstruction(url: &str, args: &Cli) {
    println!(
        r#"
  ___       _                                 _   __  __
 |_ _|_ __ | |_ _ __ ___  ___ _ __   ___  ___| |_|  \/  | ___
  | || '_ \| __| '__/ _ \/ __| '_ \ / _ \/ __| __| |\/| |/ _ \
  | || | | | |_| | | (_) \__ \ |_) |  __/ (__| |_| |  | |  __/
 |___|_| |_|\__|_|  \___/|___/ .__/ \___|\___|\__|_|  |_|\___|
                              |_|
    GraphQL Schema Reconstruction via Field Suggestion Analysis
"#
    );

    println!("[*] Target: {}", url);
    println!("[*] Output SDL: {}", args.output);
    println!("[*] Output JSON: {}", args.json_output);
    println!("[*] Request delay: {}ms", args.delay);
    println!("[*] Max depth: {}", args.depth);
    println!();

    let client = Arc::new(GraphQLClient::new(
        url,
        &args.user_agent,
        args.delay,
        args.auth.clone(),
    ));
    let schema = Arc::new(Mutex::new(ReconstructedSchema::new()));

    let walker = TypeWalker::new(client, schema.clone(), args.depth);

    if let Err(e) = walker.run().await {
        eprintln!("[!] Error during reconstruction: {}", e);
        std::process::exit(1);
    }

    let schema = schema.lock().await;

    // Output SDL
    let sdl = schema.to_sdl();
    if let Err(e) = std::fs::write(&args.output, &sdl) {
        eprintln!("[!] Failed to write SDL file: {}", e);
    } else {
        println!("\n[+] SDL schema written to: {}", args.output);
    }

    // Output JSON
    let json = serde_json::to_string_pretty(&*schema).unwrap();
    if let Err(e) = std::fs::write(&args.json_output, &json) {
        eprintln!("[!] Failed to write JSON file: {}", e);
    } else {
        println!("[+] JSON discovery data written to: {}", args.json_output);
    }

    // Summary
    println!("\n[+] Reconstruction Summary:");
    println!("    Types discovered: {}", schema.types.len());
    for (type_name, typ) in &schema.types {
        println!("      {} ({} fields)", type_name, typ.fields.len());
        for field in typ.fields.values() {
            let type_str = match &field.type_name {
                Some(t) => {
                    if field.is_list {
                        format!("[{}]", t)
                    } else {
                        t.clone()
                    }
                }
                None => "scalar".to_string(),
            };
            println!("        - {}: {}", field.name, type_str);
        }
    }
    println!(
        "    Total discovery probes: {}",
        schema.discovery_log.len()
    );
}

#[cfg(feature = "poc")]
async fn run_poc_mode(args: &Cli) {
    println!(
        r#"
  ___       _                                 _   __  __
 |_ _|_ __ | |_ _ __ ___  ___ _ __   ___  ___| |_|  \/  | ___
  | || '_ \| __| '__/ _ \/ __| '_ \ / _ \/ __| __| |\/| |/ _ \
  | || | | | |_| | | (_) \__ \ |_) |  __/ (__| |_| |  | |  __/
 |___|_| |_|\__|_|  \___/|___/ .__/ \___|\___|\__|_|  |_|\___|
                              |_|
    GraphQL Schema Reconstruction — PoC Mode
"#
    );

    println!("[*] Starting local GraphQL server with introspection DISABLED...");

    let (url, shutdown_tx) = match poc::start_poc_server().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[!] Failed to start PoC server: {}", e);
            std::process::exit(1);
        }
    };

    println!("[+] PoC server running at: {}", url);
    println!("[*] Beginning schema reconstruction...\n");

    // Get the real schema for comparison
    let real_sdl = poc::real_schema_sdl();

    // Run reconstruction against local server
    let client = Arc::new(GraphQLClient::new(
        &url,
        &args.user_agent,
        10, // fast probing for PoC
        None,
    ));
    let schema = Arc::new(Mutex::new(ReconstructedSchema::new()));
    let walker = TypeWalker::new(client, schema.clone(), args.depth);

    if let Err(e) = walker.run().await {
        eprintln!("[!] Error during reconstruction: {}", e);
        let _ = shutdown_tx.send(());
        std::process::exit(1);
    }

    let schema = schema.lock().await;
    let reconstructed_sdl = schema.to_sdl();

    // Write outputs
    if let Err(e) = std::fs::write(&args.output, &reconstructed_sdl) {
        eprintln!("[!] Failed to write SDL file: {}", e);
    } else {
        println!("\n[+] Reconstructed SDL written to: {}", args.output);
    }

    let json = serde_json::to_string_pretty(&*schema).unwrap();
    if let Err(e) = std::fs::write(&args.json_output, &json) {
        eprintln!("[!] Failed to write JSON file: {}", e);
    } else {
        println!("[+] JSON discovery data written to: {}", args.json_output);
    }

    // Print comparison
    poc::print_comparison(&real_sdl, &reconstructed_sdl);

    // Shut down PoC server
    let _ = shutdown_tx.send(());
}
