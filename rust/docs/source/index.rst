=============================
MCP-Agent Documentation
=============================

Welcome to the official documentation for MCP-Agent, a Rust implementation of the Model Context Protocol (MCP).

The Model Context Protocol (MCP) provides a standardized way for AI models to communicate with their environments, offering better structure, context management, and tool usage capabilities.

.. toctree::
   :maxdepth: 2
   :caption: Contents:

   requirements
   architecture
   project
   rust-migration
   changelog

Overview
========

MCP-Agent is a framework for building AI agents with these key features:

* **Standardized Communication**: Implements the MCP specification for consistent model-environment interactions
* **Asynchronous Processing**: Built on Rust's async/await for efficient concurrent operations
* **Workflow Engine**: Supports complex task orchestration with various workflow patterns
* **Human-in-the-loop**: Integrates human input seamlessly into agent workflows
* **LLM Integration**: Connect to various LLM providers through a unified interface
* **Telemetry & Metrics**: Comprehensive monitoring and debugging capabilities

Getting Started
===============

Installation
------------

Add the MCP-Agent to your Cargo.toml:

.. code-block:: toml

   [dependencies]
   mcp-agent = "0.1.0"

Basic Usage
-----------

Here's a simple example of using the MCP-Agent:

.. code-block:: rust

   use mcp_agent::prelude::*;
   
   #[tokio::main]
   async fn main() -> Result<(), Box<dyn std::error::Error>> {
       // Initialize agent with default configuration
       let agent = Agent::new(None);
       
       // Execute a task
       let result = agent.execute_task("hello_world", serde_json::json!({}), None).await?;
       
       println!("Task result: {:?}", result);
       Ok(())
   }

Requirements and Architecture Overview
=====================================

.. needtable::
   :columns: id;title;status;tags
   :filter: type == "requirement"
   :style: table

.. needtable::
   :columns: id;title;status;links
   :filter: type == "arch"
   :style: table

Project Status
=============

.. needtable::
   :filter: status == "open"
   :columns: id;title;status;type;tags
   :style: table

Indices and Tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search` 