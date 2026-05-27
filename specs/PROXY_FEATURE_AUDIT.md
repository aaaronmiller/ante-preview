# Claude Code Proxy → Ante Agent: Feature Audit

**Source:** `/home/cheta/code/claude-code-proxy/`
**Target:** Rust-based ante agent
**Goal:** Identify which proxy features to fold into the agent vs leave as standalone

---

## INTEGRATE (high value, bundle into agent)

### 1. Multi-Provider Cascade (Fallback Chain)
**File:** `src/core/circuit_breaker.py`, `src/core/model_router.py`, `config/proxy_chain.json`
**What it does:** On provider/model failure, automatically tries the next model in the cascade list. Configurable per tier (big/middle/small) and per slot (background, long_context, image, web_search).
**Config vars:**
- `MODEL_CASCADE=true`
- Per-assignment `cascade: ["model1:free", "model2:free"]` array
- `MODEL_CASCADE_DAILY_LIMIT=0` per-model per-day cap
**Why integrate:** Eliminates the Pi limitation of one-model-at-a-time. Single agent uses deepseek-v4-flash on OCGo → OCZen → OpenRouter → Ollama Cloud → Kilo in order.

### 2. Task-Specific Routing Slots
**File:** `src/core/model_router.py`, `config/proxy_chain.json` (`router` block)
**What it does:** Routes requests to specific models based on use-case signals:
- `default` → general (BIG tier)
- `background` → cheap/fast model
- `think` → reasoning-heavy model
- `long_context` → high-context-window model (>60K token threshold)
- `web_search` → cheap toolcaller for web tools
- `image` → vision-capable model
- Custom Python/JS router script for advanced logic
**Config vars:**
- `ROUTER_BACKGROUND=nvidia/nemotron-nano-9b-v2:free`
- `ROUTER_THINK=nvidia/nemotron-3-super-120b-a12b:free`
- `ROUTER_LONG_CONTEXT=minimax/minimax-m2.5:free`
- `ROUTER_LONG_CONTEXT_THRESHOLD=60000`
- `ROUTER_WEB_SEARCH=`
- `ROUTER_IMAGE=qwen/qwen2.5-vl-72b-instruct:free`
**Why integrate:** This is the key feature you asked about. Single agent with bash → different model than web_search → different model than large_context.

### 3. Circuit Breakers
**File:** `src/core/circuit_breaker.py`
**What it does:** Per-model state machine: CLOSED → OPEN (on N failures) → HALF_OPEN (cooldown) → probe → CLOSED. Persisted to disk so state survives restarts. Prevents hammering dead endpoints.
**Config vars:**
- `CB_FAILURE_THRESHOLD=3` failures before opening
- `CB_SUCCESS_THRESHOLD=1` successes to close half-open
- `CB_TIMEOUT_SECONDS=300` cooldown before retry
**Why integrate:** Without this, a dead model kills the agent session.

### 4. Usage Tracking (SQLite)
**File:** `src/core/config.py` (config.usage_tracking_db_path), `src/utils/request_logger.py`
**What it does:** Logs every request to SQLite: model, input_tokens, output_tokens, cost, duration_ms, status, error, timestamp. Powers analytics, billing, and the dashboard.
**Config vars:**
- `TRACK_USAGE=true`
- `USAGE_TRACKING_DB_PATH=usage_tracking.db`
**DB Schema (core):**
```sql
CREATE TABLE api_requests (
  id INTEGER PRIMARY KEY,
  timestamp TEXT,
  model TEXT,
  input_tokens INTEGER,
  output_tokens INTEGER,
  cost REAL,
  duration_ms INTEGER,
  status TEXT,
  error TEXT,
  request_count INTEGER DEFAULT 1
);
```
**Why integrate:** Gives you cost tracking per model/session and data for the refinement loop.

### 5. Semantic Cache
**File:** `src/services/semantic_cache.py`
**What it does:** Two-level cache (exact SHA-256 + fuzzy SimHash) that skips provider calls for near-duplicate prompts. Zero ML deps — pure Python SimHash. 256-entry LRU. Configurable threshold.
**Config vars:**
- `SEMANTIC_CACHE_ENABLED=true`
- `SEMANTIC_CACHE_THRESHOLD=0.97`
- `SEMANTIC_CACHE_SIZE=256`
- `SEMANTIC_CACHE_TTL=3600`
**Why integrate:** Agentic loops produce near-identical prompts (same system prompt, different tool output). Caching eliminates 40-60% of redundant API calls.

### 6. Compression / Tool Schema Stripping
**File:** `src/core/pipeline.py` (presumably, the TOOL_SCHEMA_STRIP logic)
**What it does:** Strips redundant fields from tool schemas, truncates descriptions to configurable max lengths. Reduces token burn on every tool-call round.
**Config vars:**
- `TOOL_SCHEMA_STRIP=true`
- `TOOL_DESC_MAX=200`
- `TOOL_PARAM_DESC_MAX=120`
**Why integrate:** Every tool call includes full JSON schemas. Stripping descriptions saves 20-40% tokens on tool-call-heavy workloads.

### 7. Token Budget / Cost Controls
**File:** `src/core/config.py` (env vars documented in .env.example)
**What it does:** Hard limits on per-request tokens, daily spend, and mid-stream output budget.
**Config vars:**
- `DAILY_TOKEN_BUDGET=0` (max tokens per UTC day)
- `PER_REQUEST_TOKEN_BUDGET=0` (max input tokens per request)
- `DAILY_COST_BUDGET=0.0` (max USD per UTC day)
- `MID_STREAM_OUTPUT_BUDGET=0` (route to cheaper model after N output tokens)
**Why integrate:** Prevents runaway costs from buggy agent loops.

---

## DON'T INTEGRATE (leave as standalone proxy)

### 8. Web UI / Dashboard
**File:** `src/dashboard/`, `src/api/web_ui.py`, `web-ui/`
**What it does:** SvelteKit web dashboard with usage charts, model health, cost breakdowns.
**Why skip:** You already have model-scan's SvelteKit dashboard. Don't duplicate.

### 9. GraphQL API
**File:** `src/api/graphql_schema.py`
**What it does:** GraphQL endpoint for querying proxy data.
**Why skip:** REST is simpler and your model-scan API already covers this.

### 10. User Management / RBAC
**File:** `src/auth/user_manager.py`, `src/api/users_rbac.py`
**What it does:** Multi-user auth, role-based access control, user quotas.
**Why skip:** Single-user agent. RBAC is irrelevant.

### 11. WebSocket Live Dashboard
**File:** `src/api/websocket_live.py`, `src/dashboard/live_dashboard.py`
**What it does:** Real-time streaming dashboard for proxy metrics.
**Why skip:** Overkill for an agent. Log to SQLite, query when needed.

### 12. Predictive Alerting
**File:** `src/services/predictive_alerting.py`, `src/api/predictive.py`
**What it does:** ML-based anomaly detection on usage patterns, predictive cost alerts.
**Why skip:** Premature for an agent. Simple SQLite queries cover what you need.

### 13. Third-Party Integrations
**File:** `src/services/integrations.py`, `src/api/integrations.py`
**What it does:** Slack, PagerDuty, webhook integrations for alerts.
**Why skip:** Bolt onto the agent later if needed; not core.

### 14. Report Generator
**File:** `src/services/report_generator.py`, `src/api/reports.py`
**What it does:** PDF/CSV report generation from usage data.
**Why skip:** Model-scan covers reporting needs.

### 15. Custom Router Script (Python/JS)
**File:** `src/core/model_router.py` (custom_router.py/js support)
**What it does:** Extensible routing via external scripts.
**Why skip:** The routing slots (6 built-in) cover your needs. If you need custom logic later, add it in Rust directly.

### 16. Headroom Proxy (Separate Process)
**File:** `config/proxy_chain.json` entry for headroom, running as separate service on port 8787.
**What it does:** Token headroom management middleware.
**Why skip:** Headroom is a separate process. The agent doesn't need a headroom proxy — it just needs to pick the right model for the right task.

---

## Summary Table

| # | Feature | Integrate? | Priority | Config Vars Needed |
|---|---------|-----------|----------|-------------------|
| 1 | Multi-provider cascade | ✅ INTEGRATE | P0 | `cascade` array per assignment |
| 2 | Task-specific routing | ✅ INTEGRATE | P0 | 6 slot vars (background, think, long_context, web_search, image) |
| 3 | Circuit breakers | ✅ INTEGRATE | P1 | 3 vars (threshold, success, timeout) |
| 4 | Usage tracking (SQLite) | ✅ INTEGRATE | P1 | 2 vars (enable, db_path) |
| 5 | Semantic cache | ✅ INTEGRATE | P2 | 4 vars (enable, size, threshold, ttl) |
| 6 | Tool schema stripping | ✅ INTEGRATE | P2 | 3 vars (enable, desc_max, param_max) |
| 7 | Token/cost budgets | ✅ INTEGRATE | P2 | 4 vars (daily_tokens, per_request, daily_cost, mid_stream) |
| 8 | Web UI dashboard | ❌ SKIP | — | — |
| 9 | GraphQL API | ❌ SKIP | — | — |
| 10 | User management/RBAC | ❌ SKIP | — | — |
| 11 | Live WebSocket dashboard | ❌ SKIP | — | — |
| 12 | Predictive alerting | ❌ SKIP | — | — |
| 13 | Third-party integrations | ❌ SKIP | — | — |
| 14 | Report generator | ❌ SKIP | — | — |
| 15 | Custom router scripts | ❌ SKIP | — | — |
| 16 | Headroom proxy | ❌ SKIP | — | — |

---

## Env Vars Needed for Integrated Features

```bash
# Cascade / Fallbacks
MODEL_CASCADE=true
MODEL_CASCADE_DAILY_LIMIT=0

# Routing Slots
ROUTER_BACKGROUND=nvidia/nemotron-nano-9b-v2:free
ROUTER_THINK=nvidia/nemotron-3-super-120b-a12b:free
ROUTER_LONG_CONTEXT=minimax/minimax-m2.5:free
ROUTER_LONG_CONTEXT_THRESHOLD=60000
ROUTER_WEB_SEARCH=
ROUTER_IMAGE=qwen/qwen2.5-vl-72b-instruct:free

# Circuit Breaker
CB_FAILURE_THRESHOLD=3
CB_SUCCESS_THRESHOLD=1
CB_TIMEOUT_SECONDS=300

# Usage Tracking
TRACK_USAGE=true
USAGE_TRACKING_DB_PATH=~/.ante/usage.db

# Semantic Cache
SEMANTIC_CACHE_ENABLED=true
SEMANTIC_CACHE_THRESHOLD=0.97
SEMANTIC_CACHE_SIZE=256
SEMANTIC_CACHE_TTL=3600

# Compression
TOOL_SCHEMA_STRIP=true
TOOL_DESC_MAX=200
TOOL_PARAM_DESC_MAX=120

# Budget Controls
DAILY_TOKEN_BUDGET=0
PER_REQUEST_TOKEN_BUDGET=0
DAILY_COST_BUDGET=0.0
MID_STREAM_OUTPUT_BUDGET=0
```

## Cascade Config Structure (for ante)

The core data structure your agent needs for routing:

```rust
struct Assignment {
    id: String,              // "primary", "background", "think", etc.
    kind: AssignmentKind,    // Tier | Slot
    model: String,           // default model for this slot
    provider: String,        // "opencode-go", "opencode-zen", "openrouter", etc.
    base_url: String,        // API endpoint
    api_key_env: String,     // env var name for the key
    enabled: bool,
    cascade: Vec<String>,    // fallback models tried in order
}

struct Router {
    default: Assignment,
    background: Assignment,
    think: Assignment,
    long_context: Assignment,
    web_search: Assignment,
    image: Assignment,
    long_context_threshold: u64,  // tokens
}
```

The `cascade` array is the key. When a model returns 429/401/500, the router tries the next model in the cascade list before erroring. Circuit breaker tracks failures per model and skips ones in OPEN state.
