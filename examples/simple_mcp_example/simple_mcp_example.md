# Simple MCP Terminal Example

This example demonstrates how to start a simple MCP mock server and connect to it using the MCP terminal client.

## Prerequisites

- The `mcp-agent` Rust crate built and compiled
- Two terminal windows or tabs

## Step 1: Start the Mock Server

In the first terminal window, navigate to the `mcp-agent` directory and run the mock server:

```bash
cd /path/to/mcp-agent
cargo run --bin mock_server
```

You should see output similar to:

```
Mock server listening on 127.0.0.1:7000
```

## Step 2: Start the MCP Terminal Client

In the second terminal window, navigate to the `mcp-agent` directory and run the terminal client:

```bash
cd /path/to/mcp-agent
cargo run --bin terminal
```

You should see output similar to:

```
MCP Agent Terminal
Type 'help' for available commands
mcp>
```

## Step 3: Connect to the Mock Server

In the terminal client, connect to the mock server using the `connect` command:

```
mcp> connect test-server 127.0.0.1:7000
```

You should see a success message:

```
Success: Connected to test-server
```

## Step 4: List Connected Servers

Verify the connection by listing all connected servers:

```
mcp> list
```

You should see:

```
Success: Connected to 1 servers: test-server
```

## Step 5: Send Messages to the Server

Send a simple message to the server:

```
mcp> send test-server echo hello world
```

You should see:

```
Success: Message sent to test-server
```

The mock server will echo back the message.

Try other predefined commands:

```
mcp> send test-server ping
mcp> send test-server json
```

## Step 6: Execute Functions

The terminal also supports executing functions with arguments:

```
mcp> exec get_status {}
```

## Step 7: Disconnect and Exit

Disconnect from the server:

```
mcp> disconnect test-server
```

You should see:

```
Success: Disconnected from test-server
```

To exit the terminal client:

```
mcp> exit
```

## Understanding the Communication

The MCP protocol uses a simple message format with:

- Message Type (Request, Response, Event, KeepAlive)
- Priority (Low, Normal, High)
- Message ID
- Optional Correlation ID
- Optional Error
- Payload

The mock server in this example handles specific keywords in messages:

- `echo`: Echoes back the message
- `ping`: Responds with "pong"
- `json`: Returns a JSON object
- `error`: Returns an error message

## Next Steps

- Implement a custom MCP server
- Create more complex request/response patterns
- Explore streaming responses with the `stream` command
