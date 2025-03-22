# MCP-Agent Next Steps

This document outlines the requirements that still need to be implemented, based on the `requirements.rst` file. The requirements are organized by priority and implementation difficulty.

## Protocol Implementation

1. **REQ-023: WebSocket/HTTP Transport** - ✅ Implemented WebSocket and HTTP transport layers for message exchange with connection management and error handling.
2. **REQ-024: JSON-RPC Batch Processing** - Add support for batch requests and responses per JSON-RPC 2.0 specification to improve throughput and reduce latency.
3. **REQ-025: Authentication and Security** - Implement authentication mechanisms (API keys, OAuth, JWT) and transport-level encryption.

## Quality Assurance

4. **REQ-022: Formal Verification** - Apply formal verification tools (KLEE, Creusot) to critical components to ensure correctness and safety properties.

## Terminal Interface

5. **REQ-026: Dual Terminal Support** - Support both console-based and web-based terminal interfaces with simultaneous interaction.
6. **REQ-027: Terminal Configuration** - Allow configuration for console terminal only, web terminal only, or both through runtime options.
7. **REQ-028: Terminal Synchronization** - Ensure input/output synchronization between console and web terminals when both are active.
8. **REQ-029: Web Terminal Security** - Implement authentication, authorization, and encryption for the web terminal interface.
9. **REQ-030: Dynamic Terminal Switching** - Support runtime enabling/disabling of web terminal without application restart.

## Graph Visualization

10. **REQ-026: Workflow Graph Visualization** - Provide interactive visualization of workflow graphs with real-time updates.
11. **REQ-027: Agent System Visualization** - Visualize agent hierarchy, relationships, and message passing with status indicators.
12. **REQ-028: LLM Integration Visualization** - Visualize LLM integration points and data flow between components.
13. **REQ-029: Human Input Visualization** - Visualize human input points in workflows with real-time status.
14. **REQ-030: Visualization UI Controls** - Add controls for toggling visualization, zooming, panning, and selecting graph elements.
15. **REQ-031: Real-Time Visualization Updates** - Ensure visualization updates with minimal latency (<100ms).
16. **REQ-032: Web Terminal Visualization Integration** - Integrate graph visualization with web terminal interface.
17. **REQ-033: Graph Visualization API** - Provide REST and WebSocket APIs for graph data access.
18. **REQ-034: Sprotty-Compatible Visualization** - Use Eclipse Sprotty for rendering interactive graphs.

## Implementation Plan

### Next Priority: JSON-RPC Batch Processing (REQ-024)

The JSON-RPC 2.0 specification includes support for batch processing, which allows multiple requests to be sent in a single message. This is important for improving throughput and reducing latency for applications that need to make multiple API calls.

#### Tasks:

- [ ] Review the JSON-RPC 2.0 specification for batch processing requirements
- [ ] Design the batch processing interface
- [ ] Implement batch request handling
- [ ] Implement batch response handling
- [ ] Add error handling for batch requests
- [ ] Write tests for batch processing
- [ ] Document the batch processing API
