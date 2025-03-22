===========================
MCP-Agent Architecture
===========================

Overview
========

The MCP-Agent is a Rust implementation of the Model Context Protocol (MCP), providing a framework for building AI agents with standardized communication patterns. This document describes the architecture of the system, including component structure, communication patterns, and key workflows.

.. contents:: Table of Contents
   :depth: 3
   :local:

System Components
================

The MCP-Agent architecture consists of the following major components:

.. arch:: MCP Protocol Layer
   :id: ARCH-001
   :status: implemented
   :tags: core;protocol
   :links: REQ-001;REQ-023;REQ-024;REQ-025
   
   Provides the core protocol implementation with JSON-RPC 2.0 specification, handling message serialization, deserialization, and transport protocols.

.. arch:: Agent System
   :id: ARCH-002
   :status: partial
   :tags: core;patterns
   :links: REQ-002;REQ-003
   
   Implements agent patterns and lifecycle management for both single agents and multi-agent orchestration.

.. arch:: Error Handling System
   :id: ARCH-003
   :status: implemented
   :tags: quality;safety
   :links: REQ-008
   
   Provides comprehensive error handling with proper propagation, logging, and recovery mechanisms across all components.

.. arch:: Concurrency Management
   :id: ARCH-004
   :status: implemented
   :tags: performance;concurrency
   :links: REQ-005
   
   Implements async/await patterns for concurrent operations, leveraging Rust's async runtime for efficient resource utilization.

.. arch:: Telemetry System
   :id: ARCH-005
   :status: implemented
   :tags: observability;telemetry
   :links: REQ-009
   
   Provides monitoring, metrics, and alerting through OpenTelemetry integration for comprehensive system observability.

.. arch:: Workflow Engine
   :id: ARCH-006
   :status: implemented
   :tags: orchestration;workflow
   :links: REQ-012
   
   Orchestrates tasks and manages workflows with proper error recovery and state management capabilities.

.. arch:: Human Input Module
   :id: ARCH-007
   :status: implemented
   :tags: interface;interaction
   :links: REQ-021
   
   Handles user interaction during workflow execution, including prompts, validation, and timeout handling.

.. arch:: Extension System
   :id: ARCH-008
   :status: implemented
   :tags: architecture;design
   :links: REQ-020
   
   Provides extension points for adding new components, models, and tools with minimal changes to the core system.

.. arch:: LLM Client
   :id: ARCH-009
   :status: partial
   :tags: integration;ai
   :links: REQ-011
   
   Manages connections to various LLM providers with standardized interfaces and proper error handling.

.. arch:: Terminal System
   :id: ARCH-010
   :status: open
   :tags: interface;terminal
   :links: REQ-026;REQ-027;REQ-028;REQ-029;REQ-030
   
   Provides a unified terminal interface layer supporting both console and web-based terminals with synchronized I/O, configurable activation, and secure remote access.

.. arch:: Terminal System
   :id: ARCH-013
   :status: implemented
   :tags: ui;terminal
   :links: REQ-020;REQ-021;REQ-022
   
   Provides both console and web-based terminal interfaces with synchronization capabilities for consistent user interaction.

.. uml:: _static/terminal_system.puml
   :alt: Terminal System

Graph Visualization System
-------------------------

.. arch:: Graph Visualization System
   :id: ARCH-016
   :status: open
   :tags: ui;visualization
   :links: REQ-026;REQ-027;REQ-028;REQ-029;REQ-030;REQ-031;REQ-032;REQ-033;REQ-034
   
   Provides interactive graph visualization for workflows, agents, and system components, with real-time updates and integration with the terminal system.

.. arch:: Graph Data Providers
   :id: ARCH-017
   :status: open
   :tags: visualization;data
   :links: REQ-026;REQ-027;REQ-028;REQ-029;ARCH-016
   
   Extracts data from MCP-Agent components (workflow engine, agent system, etc.) and converts it to standardized graph models for visualization.

.. arch:: Graph Manager
   :id: ARCH-018
   :status: open
   :tags: visualization;state
   :links: REQ-031;ARCH-016;ARCH-017
   
   Manages graph state and propagates updates to subscribers, serving as the central coordinator for the visualization system.

.. arch:: Graph Visualization API
   :id: ARCH-019
   :status: open
   :tags: visualization;api
   :links: REQ-033;ARCH-016;ARCH-018
   
   Provides REST and WebSocket endpoints for accessing graph data, allowing clients to retrieve graphs and subscribe to real-time updates.

.. arch:: Sprotty Integration
   :id: ARCH-020
   :status: open
   :tags: visualization;frontend
   :links: REQ-034;ARCH-016;ARCH-019
   
   Implements client-side integration with Eclipse Sprotty for interactive graph rendering, with model mapping between backend data and Sprotty-compatible formats.

.. uml:: graph-visualization-system

    @startuml
    
    package "MCP-Agent Core" {
        [WorkflowEngine] as WE
        [AgentSystem] as AS
        [LlmProviders] as LLM
        [HumanInputSystem] as HIS
    }
    
    package "Graph Visualization System" {
        [GraphManager] as GM
        
        package "Data Providers" {
            [WorkflowGraphProvider] as WGP
            [AgentGraphProvider] as AGP
            [LlmIntegrationProvider] as LIP
            [HumanInputProvider] as HIP
        }
        
        package "Backend API" {
            [GraphRestAPI] as GAPI
            [GraphWebSocketAPI] as GWSAPI
        }
    }
    
    package "Terminal System" {
        [WebTerminal] as WT
        [ConsoleTerminal] as CT
    }
    
    package "Web Client" {
        [SprottyRenderer] as SR
        [VisualizationControls] as VC
        [WebSocket Client] as WSC
    }
    
    WE --> WGP : "Provides data"
    AS --> AGP : "Provides data"
    LLM --> LIP : "Provides data"
    HIS --> HIP : "Provides data"
    
    WGP --> GM : "Registers graphs"
    AGP --> GM : "Registers graphs"
    LIP --> GM : "Registers graphs"
    HIP --> GM : "Registers graphs"
    
    GM --> GAPI : "Provides data"
    GM --> GWSAPI : "Provides updates"
    
    GAPI --> SR : "Initial graph data"
    GWSAPI --> WSC : "Real-time updates"
    WSC --> SR : "Updates visualization"
    
    WT --> WSC : "Embeds"
    WT --> SR : "Embeds"
    WT --> VC : "Embeds"
    
    @enduml

.. uml:: graph-data-models

    @startuml
    
    package "Core Graph Model" {
        class Graph {
            +id: String
            +name: String
            +graph_type: String
            +nodes: Vec<GraphNode>
            +edges: Vec<GraphEdge>
            +properties: HashMap<String, Value>
        }
        
        class GraphNode {
            +id: String
            +name: String
            +node_type: String
            +status: String
            +properties: HashMap<String, Value>
        }
        
        class GraphEdge {
            +id: String
            +source: String
            +target: String
            +edge_type: String
            +properties: HashMap<String, Value>
        }
        
        class GraphUpdate {
            +graph_id: String
            +update_type: GraphUpdateType
            +graph: Option<Graph>
            +node: Option<GraphNode>
            +edge: Option<GraphEdge>
        }
        
        enum GraphUpdateType {
            +FullUpdate
            +NodeAdded
            +NodeUpdated
            +NodeRemoved
            +EdgeAdded
            +EdgeUpdated
            +EdgeRemoved
        }
    }
    
    package "Sprotty Model" {
        class SprottyRoot {
            +id: String
            +type: String
            +children: Vec<SprottyElement>
        }
        
        class SprottyNode {
            +id: String
            +cssClasses: Option<Vec<String>>
            +layout: Option<String>
            +position: Option<SprottyPoint>
            +size: Option<SprottyDimension>
            +children: Vec<SprottyElement>
            +properties: HashMap<String, Value>
        }
        
        class SprottyEdge {
            +id: String
            +sourceId: String
            +targetId: String
            +cssClasses: Option<Vec<String>>
            +routingPoints: Option<Vec<SprottyPoint>>
            +children: Vec<SprottyElement>
            +properties: HashMap<String, Value>
        }
    }
    
    Graph o--> GraphNode : "contains"
    Graph o--> GraphEdge : "contains"
    GraphUpdate --> Graph : "may contain"
    GraphUpdate --> GraphNode : "may contain"
    GraphUpdate --> GraphEdge : "may contain"
    GraphUpdate --> GraphUpdateType : "has type"
    
    SprottyRoot o--> SprottyNode : "may contain"
    SprottyRoot o--> SprottyEdge : "may contain"
    
    @enduml

.. uml:: graph-data-flow

    @startuml
    
    actor User
    participant WebClient
    participant WebTerminal
    participant GraphWebSocketAPI
    participant GraphRestAPI
    participant GraphManager
    participant DataProviders
    participant MCPComponents
    
    == Initialization ==
    
    User -> WebTerminal: Access web terminal
    WebTerminal -> GraphRestAPI: Request available graphs
    GraphRestAPI -> GraphManager: Get graph list
    GraphManager --> GraphRestAPI: Return graph IDs
    GraphRestAPI --> WebTerminal: Graph IDs
    WebTerminal -> WebClient: Show visualization toggle
    
    == Connect to Data Stream ==
    
    User -> WebClient: Toggle visualization
    WebClient -> GraphWebSocketAPI: Open WebSocket connection
    GraphWebSocketAPI -> GraphManager: Subscribe to updates
    GraphManager --> GraphWebSocketAPI: Confirm subscription
    WebClient -> GraphRestAPI: Request initial graph data
    GraphRestAPI -> GraphManager: Get graph data
    GraphManager --> GraphRestAPI: Return graph data
    GraphRestAPI --> WebClient: Graph data
    WebClient -> WebClient: Render graph
    
    == Real-time Updates ==
    
    MCPComponents -> DataProviders: State change notification
    DataProviders -> GraphManager: Update graph data
    GraphManager -> GraphManager: Process update
    GraphManager -> GraphWebSocketAPI: Broadcast update
    GraphWebSocketAPI -> WebClient: Send update
    WebClient -> WebClient: Update visualization
    
    @enduml

Component Diagram
----------------

.. uml:: _static/component_diagram.puml
   :alt: Component Diagram

Module Dependencies
------------------

.. uml:: _static/module_dependencies.puml
   :alt: Module Dependencies Diagram

.. spec:: MCP Protocol Specification
   :id: SPEC-001
   :status: implemented
   :tags: protocol;specification
   :links: REQ-001;ARCH-001
   
   The Model Context Protocol specification defines the standards for communication between AI models and software components through a JSON-RPC interface.

Core Communication Flow
=======================

The MCP protocol enables communication between AI models and software components through a standardized JSON-RPC interface. The sequence diagram below illustrates the basic communication pattern:

.. uml:: _static/core_communication_flow.puml
   :alt: Core Communication Flow Diagram

Workflow Execution
=================

The workflow engine orchestrates the execution of complex tasks with the following sequence:

.. uml:: _static/workflow_execution.puml
   :alt: Workflow Execution Diagram

Agent Patterns
=============

The MCP-Agent framework supports various agent patterns as described in the "Building Effective Agents" paper:

.. uml:: _static/agent_patterns.puml
   :alt: Agent Patterns Diagram

Error Handling
=============

The error handling architecture provides a consistent approach to error management:

.. uml:: _static/error_handling.puml
   :alt: Error Handling Diagram

Sequence Diagrams
================

Event-Driven Workflow
--------------------

.. uml:: _static/event_driven_workflow.puml
   :alt: Event-Driven Workflow Diagram

Human Input Workflow
-------------------

.. uml:: _static/human_input_workflow.puml
   :alt: Human Input Workflow Diagram

JSON-RPC Communication
---------------------

.. uml:: _static/jsonrpc_communication.puml
   :alt: JSON-RPC Communication Diagram

Data Flow Architecture
=====================

The data flow between components follows a consistent pattern:

.. uml:: _static/data_flow_architecture.puml
   :alt: Data Flow Architecture Diagram

Deployment Architecture
======================

The MCP-Agent can be deployed in various configurations:

.. uml:: _static/deployment_architecture.puml
   :alt: Deployment Architecture Diagram

Security Architecture
===================

The security architecture ensures proper data protection and access control:

.. uml:: _static/security_architecture.puml
   :alt: Security Architecture Diagram

Extension Points
==============

The MCP-Agent architecture provides several extension points:

.. uml:: _static/extension_points.puml
   :alt: Extension Points Diagram

Terminal Interface Architecture
==============================

The dual terminal system allows for both local console and remote web-based interaction with the MCP-Agent:

.. uml:: _static/terminal_system_architecture.puml
   :alt: Terminal System Architecture Diagram

Terminal I/O Flow
----------------

.. uml:: _static/terminal_io_flow.puml
   :alt: Terminal I/O Flow Diagram

Architecture-Requirements Traceability
======================================

.. needtable::
   :filter: type == "arch"
   :columns: id;title;status;links
   :style: table

Conclusion
==========

The MCP-Agent architecture provides a robust, extensible framework for implementing AI agents using the Model Context Protocol. The modular design allows for customization of various components while maintaining a consistent interface for developers.

The Rust implementation leverages the language's strengths in performance, safety, and concurrency, providing a solid foundation for building production-ready AI agent systems. 