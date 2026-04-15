# Example 4: HTTP API

**Interface:** HTTP REST API (curl)
**Time to run:** ~30 seconds
**Extra dependency:** `jq` (optional, for pretty-printing JSON)

## What You'll Learn

- How to start Clear Memory as an **HTTP server**
- **Every REST endpoint** with real curl commands and JSON responses
- How to integrate Clear Memory into any web app, script, or CI pipeline
- That CLI and HTTP work against the **same data store**

## Run It

```bash
cd examples/04-http-api
./run.sh
```

## What Happens

1. Seeds the database with 8 sprint retrospective memories via CLI import
2. Starts the HTTP server on port 18080
3. Checks `/health` endpoint
4. Stores a new memory via `POST /v1/retain`
5. Gets corpus status via `GET /v1/status`
6. Searches via `POST /v1/recall` for "deployment strategy"
7. Expands the first result via `GET /v1/expand/:id`
8. Lists streams and tags
9. Forgets a memory via `POST /v1/forget`
10. Stops the server

Every curl command shows both the request and the JSON response.

## Using in Your Own Code

```bash
# Store a memory
curl -X POST http://localhost:8080/v1/retain \
  -H "Content-Type: application/json" \
  -d '{"content": "your memory here", "tags": ["team:platform"]}'

# Search
curl -X POST http://localhost:8080/v1/recall \
  -H "Content-Type: application/json" \
  -d '{"query": "what database do we use"}'
```
