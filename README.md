# IntrospectMe

GraphQL schema reconstruction tool that exploits "Did you mean X?" field suggestion errors to fully reconstruct schemas **without ever sending introspection queries**.

## How It Works

GraphQL engines return suggestion errors like `Cannot query field "xuser" on type "Query". Did you mean "user"?` even when introspection is disabled. IntrospectMe systematically exploits this behavior:

1. Sends queries with intentionally wrong field names derived from a wordlist
2. Parses "Did you mean...?" suggestions from error responses
3. Detects object types via "must have a selection of subfields" errors
4. Recursively probes subfields on discovered object types
5. Builds a complete schema graph and outputs valid `.graphql` SDL

All requests look identical to normal failed GraphQL queries. No `__schema` or `__type` introspection queries are ever sent.

## Installation

```bash
cargo build --release
```

The binary is at `target/release/introspectme`.

## Usage

### Against a target endpoint

```bash
introspectme --url https://target.com/graphql --output schema.graphql
```

### PoC mode (local demo)

Spins up a local GraphQL server with introspection disabled and runs reconstruction against it:

```bash
introspectme --poc
```

### Full options

```
introspectme [OPTIONS]

Options:
    --url <URL>              Target GraphQL endpoint URL
    --output <FILE>          Output SDL schema file [default: schema.graphql]
    --json-output <FILE>     Output raw JSON discovery data [default: discovered.json]
    --delay <MS>             Delay between requests in ms [default: 100]
    --user-agent <UA>        Custom User-Agent header
    --depth <N>              Max recursion depth for type walking [default: 10]
    --auth <HEADER>          Authorization header value (e.g., "Bearer token123")
    --poc                    Run PoC mode with local server
```

### Environment variables

- `INTROSPECTME_DEBUG=1` — Print all GraphQL error messages to stderr for debugging

## Architecture

```
src/
├── main.rs       — CLI entry point, orchestration
├── cli.rs        — Argument parsing (clap)
├── wordlist.rs   — Built-in wordlist + typo mutation engine
├── client.rs     — Async HTTP client, error response parser
├── walker.rs     — Recursive type walker with visited tracking
├── schema.rs     — Schema model, SDL/JSON output generation
└── poc.rs        — PoC mode: local GraphQL server (actix-web + async-graphql)
```

## Wordlist & Mutation Engine

The built-in wordlist contains 170+ common GraphQL field names across categories:
- Identity/Auth: `user`, `email`, `token`, `role`, `session`, ...
- E-commerce: `product`, `order`, `cart`, `payment`, `price`, ...
- Pagination: `cursor`, `pageInfo`, `totalCount`, `edges`, ...
- Relay: `node`, `edge`, `connection`, ...

For each word, the mutation engine generates typo variants to maximize suggestion hits:
- Prefix: `xuser`
- Drop last char: `use`
- Swap chars: `suer`
- Add suffixes: `users`, `userId`, `userBy`, `userList`
- Case variants: `User`

## Output

### SDL (`.graphql`)

```graphql
schema {
  query: QueryRoot
}

type QueryRoot {
  user: User
  users: [User]
  product: Product
}

type User {
  name: String
  email: String
  role: String
  profile: Profile
}
```

### JSON (`discovered.json`)

Contains the full schema model including types, fields, type mappings, and the raw discovery log of every suggestion received.

## PoC Results

Running `--poc` against the built-in test schema (6 types, 34 fields):

```
                Real    Reconstructed
  Types:           6       6
  Fields:         34      27
```

All 6 types discovered. The 7 missing fields are primarily `id` fields (too short to trigger suggestions in most GraphQL engines).

## Limitations

- Fields with very short names (`id`, `me`) may not trigger suggestions
- Argument types and nullability cannot be determined from error messages
- Scalar types are inferred from naming conventions (heuristic)
- Very large schemas require more time due to the probing approach
- Some GraphQL implementations may not return "Did you mean" suggestions

## Security Context

This tool is intended for authorized security testing, penetration testing engagements, and security research. It demonstrates that disabling introspection alone is insufficient to protect GraphQL schema information.
