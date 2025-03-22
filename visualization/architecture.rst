===========================================
Graph Visualization System Architecture
===========================================

Overview
--------

The graph visualization system extends the MCP-Agent terminal with interactive visual representations 
of workflows, agents, and other components. This document describes the system architecture, including 
components, data flow, and integration points.

System Components
----------------

The architecture consists of the following major components:

1. Data Providers
~~~~~~~~~~~~~~~~

The data providers extract graph data from the MCP-Agent system components and convert it to the
standardized graph model format. Each provider is responsible for a specific domain:

* **WorkflowGraphProvider**: Extracts graph data from the workflow engine, including workflows, tasks, and dependencies
* **AgentGraphProvider**: Creates a graph representation of the agent system, including connections and status
* **HumanInputGraphProvider**: Represents human input components and their integration points
* **LlmIntegrationGraphProvider**: Visualizes LLM providers and their connections

2. Graph Manager
~~~~~~~~~~~~~~~

The central component that manages graph data and propagates updates:

* Maintains the current state of all graphs
* Provides API for registering new graphs and updating existing ones
* Manages subscriptions to graph updates
* Handles serialization of graph data for transmission

3. Backend API
~~~~~~~~~~~~~

REST and WebSocket endpoints for accessing graph data:

* ``/api/graph`` - List all available graphs
* ``/api/graph/:id`` - Get specific graph data 
* ``/api/graph/ws`` - WebSocket endpoint for real-time updates

4. Frontend Components
~~~~~~~~~~~~~~~~~~~~~

Client-side components that render and interact with the graph:

* **Sprotty Integration**: Eclipse Sprotty framework for graph rendering
* **Model Mapping**: Converts backend graph data to Sprotty-compatible format
* **UI Controls**: Toggle buttons, layout controls, and visualization options
* **Event Handlers**: Process user interactions and WebSocket updates

Data Models
----------

1. Graph Model
~~~~~~~~~~~~~

The core data model for representing graph structures:

* **Graph**: Top-level container with metadata and collections of nodes and edges
* **GraphNode**: Represents a component in the system (workflow, task, agent, etc.)
* **GraphEdge**: Represents a relationship between two nodes
* **GraphUpdate**: Represents a change to the graph (add/update/remove node or edge)

2. Sprotty Model
~~~~~~~~~~~~~~~

Frontend model compatible with the Sprotty rendering engine:

* **SprottyRoot**: Top-level element containing the entire graph
* **SprottyNode**: Visual representation of a graph node
* **SprottyEdge**: Visual representation of a graph edge
* **SprottyLabel**: Text element for displaying node/edge information
* **SprottyCompartment**: Container for grouping related elements

Data Flow
--------

1. Data Extraction
~~~~~~~~~~~~~~~~~

1. Data providers connect to MCP-Agent components
2. Providers extract relevant data and convert to graph models
3. Graph manager registers the initial graph data
4. Providers set up listeners for state changes

2. Data Updates
~~~~~~~~~~~~~~

1. Component state changes (e.g., workflow task starts executing)
2. Data provider detects the change and updates the graph
3. Graph manager broadcasts the update to subscribers
4. WebSocket connections receive the update

3. Visualization Rendering
~~~~~~~~~~~~~~~~~~~~~~~~~

1. Client requests initial graph data via REST API
2. Client establishes WebSocket connection for updates
3. Client renders graph using Sprotty
4. Real-time updates are applied to the visualization

Integration Points
----------------

1. Terminal System Integration
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

* Graph visualization is integrated with the web terminal UI
* Terminal and visualization can be used concurrently
* Authentication and security models are shared

2. Workflow Engine Integration
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

* WorkflowGraphProvider connects to the workflow engine
* Graph updates reflect real-time workflow state
* Task execution is visualized with highlighting

3. Agent System Integration
~~~~~~~~~~~~~~~~~~~~~~~~~~

* AgentGraphProvider tracks agent connections and status
* Agent messages and interactions are visualized
* Agent hierarchy is represented in the graph structure

4. UI Integration
~~~~~~~~~~~~~~~~

* Toggle button in terminal UI to show/hide visualization
* Optional split-screen view for terminal and visualization
* Shared styling and design language

Deployment Model
--------------

The graph visualization system is deployed as part of the MCP-Agent package:

* Backend components are compiled into the Rust binary
* Frontend components are embedded in the web terminal HTML/JS
* No additional services or dependencies required
* Uses the same network ports and interfaces as the terminal

Security Considerations
---------------------

1. Authentication
~~~~~~~~~~~~~~~~

* Graph API uses the same authentication as the terminal
* JWT validation for all API endpoints
* WebSocket connections validate authentication on connection

2. Authorization
~~~~~~~~~~~~~~~

* Access control follows terminal system permissions
* Visualization data matches terminal access level
* No additional privileges required for visualization

3. Data Protection
~~~~~~~~~~~~~~~~~

* Sensitive data is filtered before visualization
* All data transfers use TLS
* Input validation for all API parameters 