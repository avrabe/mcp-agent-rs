.. MCP Agent documentation master file, created by
   sphinx-quickstart on Thu Aug 17 09:30:44 2023.
   You can adapt this file completely to your liking, but it should at least
   contain the root `toctree` directive.

Welcome to MCP Agent's documentation!
====================================

.. toctree::
   :maxdepth: 2
   :caption: Contents:
   
   requirements
   changelog
   rust-migration
   project

.. toctree::
   :maxdepth: 2
   :caption: Technical Documentation
   
   architecture
   visualization/requirements
   visualization/architecture

.. toctree::
   :maxdepth: 2
   :caption: API Reference

Overview
========

MCP-Agent is a Rust implementation of the Model Context Protocol (MCP), which is a framework
for building AI-powered agents and workflows. It provides a unified interface for integrating 
various AI models, human input, and external tools.

The MCP protocol defines how agents interact with each other and with the world, 
establishing a clear context management system to maintain state across interactions.

Key Features
-----------

* **Agent Management**: Create, connect, and manage MCP agents
* **Workflow Orchestration**: Define and execute complex workflows with dependencies
* **LLM Integration**: Connect to various LLM providers through a unified interface
* **Human Input**: Integrate human feedback and decisions into agent workflows
* **Terminal Interfaces**: Both console and web-based terminal interfaces
* **Visualization Tools**: Graph-based visualization of workflows and agent systems

Getting Started
==============

Installation
-----------

To install MCP-Agent:

.. code-block:: bash

   cargo add mcp-agent

Basic Usage
----------

.. code-block:: rust

   use mcp_agent::prelude::*;
   
   #[tokio::main]
   async fn main() -> Result<()> {
       // Create a simple agent
       let mut agent = Agent::new();
       
       // Connect to a provider
       agent.connect("provider_url").await?;
       
       // Send a message
       let response = agent.send("Hello, world!").await?;
       
       println!("Response: {}", response);
       
       Ok(())
   }

Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search` 