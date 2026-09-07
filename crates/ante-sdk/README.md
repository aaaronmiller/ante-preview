# ante-sdk

Rust SDK for Ante.

A `Client` is one connection to an Ante host. It sends `Op`s and receives
`EventMsg`s; which session it drives is decided by the ops sent over it, never
by how it was opened.

```rust
let client = ante_sdk::connect("stdio".parse()?, ConnectOptions::default()).await?;
```

An `Endpoint` names where a host is reachable, never a session:

| Endpoint | The client | Host lifetime |
| --- | --- | --- |
| `stdio` | spawns `ante serve --stdio` as its own child | the connection's |
| `unix:<path>` | dials the socket file of an `ante serve --sock` host | someone else's |
| `ws://<addr>` | dials a WebSocket server (not yet connectable) | someone else's |

A process that hosts sessions itself obtains the same `Client` type from its
host directly; the in-process channel carries the same wire types the remote
codecs serialize.

The `claude` module is unrelated: it drives Claude Code as a child process.

## Claude Code

Rust client for [Claude Code](https://code.claude.com/docs/en/cli-reference).

Other SDKs:
- [Python SDK](https://github.com/anthropics/claude-agent-sdk-python).

### Long-lived agent runtime

The SDK turns Claude Code into a long-lived agent runtime. Instead of
launching separate `claude -p "…"` invocations and stitching sessions back
together with `--resume`, `Claude::connect` spawns a single subprocess that
stays alive across turns. Call `query` or `send_user_text` as many times as
you need — the underlying process, conversation history, and tool state
persist for the lifetime of the connection. This makes it straightforward to
build multi-turn agents, orchestration loops, and interactive applications on
top of Claude Code.

### What it provides

- `Claude::connect(options)` then `query` / `send_user_text` for sessions;
  call `shutdown` when done
- typed control helpers (`set_model`, `set_permission_mode`, `interrupt`,
  `rewind_files`, `get_mcp_status`, …)
- typed `ClaudeMessage` parsing for `assistant`, `user`, `system`, `result`,
  `stream_event`, and control protocol frames
- low-level `Stdio` transport for raw newline-delimited JSON access

### Usage

```rust
use agent_sdk::claude::{Claude, ClaudeMessage, ClaudeOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = ClaudeOptions {
        model: Some("claude-sonnet-4-5".to_string()),
        ..ClaudeOptions::default()
    };
    let mut client = Claude::connect(options).await?;
    let response = client.query("Summarize this repo").await?;
    client.shutdown().await?;

    for message in response.messages {
        if let ClaudeMessage::Assistant(message) = message
            && let Some(text) = message.text()
        {
            println!("{text}");
        }
    }

    Ok(())
}
```

### Example

`examples/claude_code.rs` covers both one-shot and interactive modes. When no
prompt is given, it starts an interactive REPL:

```bash
cargo run --example claude_code -- "What is 2 + 2?"
cargo run --example claude_code -- --model claude-sonnet-4-5 "Summarize this repo"
cargo run --example claude_code -- --cli-path /path/to/claude "Hello"
cargo run --example claude_code --                              # REPL
cargo run --example claude_code -- --model claude-sonnet-4-5    # REPL with model
```

### Notes

- The SDK shells out to the external `claude` executable; it does not bundle
  Claude Code.
- Ante includes hook callback plumbing and built-in MCP server support; this SDK
  still uses the external `claude` executable for the core agent transport.
- Additional agent runtimes may be added in the future behind the same SDK
  surface.
