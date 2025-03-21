import os
import sys
import platform

sys.path.insert(0, os.path.abspath("../.."))

project = "MCP-Agent"
copyright = "2024, MCP-Agent Contributors"
author = "MCP-Agent Contributors"
release = "0.1.0"

extensions = [
    "sphinx.ext.autodoc",
    "sphinx.ext.viewcode",
    "sphinx.ext.napoleon",
    "sphinx_needs",
    "myst_parser",
    "sphinxcontrib.plantuml",
    "sphinxcontrib_rust",
]

templates_path = ["_templates"]
exclude_patterns = []

html_theme = "sphinx_book_theme"
html_static_path = ["_static"]

# PlantUML configuration
# Using the installed plantuml executable
plantuml = "plantuml"
plantuml_output_format = "svg"

# Make PlantUML work cross-platform
if platform.system() == "Windows":
    # Windows may need the full path to the plantuml.jar or plantuml.bat
    plantuml = os.environ.get("PLANTUML_PATH", "plantuml")
elif platform.system() == "Darwin":  # macOS
    # macOS typically uses Homebrew installation
    plantuml = os.environ.get("PLANTUML_PATH", "plantuml")
elif platform.system() == "Linux":
    # Linux installation path
    plantuml = os.environ.get("PLANTUML_PATH", "plantuml")

# Allow customization through environment variables
plantuml_output_format = os.environ.get("PLANTUML_FORMAT", "svg")

# Sphinx-needs configuration
needs_types = [
    dict(
        directive="req",
        title="Requirement",
        prefix="REQ-",
        color="#BFD8D2",
        style="node",
    ),
    dict(
        directive="spec",
        title="Specification",
        prefix="SPEC-",
        color="#FEDCD2",
        style="node",
    ),
    dict(
        directive="arch",
        title="Architecture",
        prefix="ARCH-",
        color="#DF744A",
        style="node",
    ),
    dict(
        directive="test",
        title="Test Case",
        prefix="TEST-",
        color="#DCB239",
        style="node",
    ),
    dict(
        directive="impl",
        title="Implementation",
        prefix="IMPL-",
        color="#9D5C63",
        style="node",
    ),
]

# Valid status values
needs_statuses = [
    dict(name="open", description="Open item", color="#FF8E00"),
    dict(name="in_progress", description="In progress", color="#FFFF00"),
    dict(name="implemented", description="Implemented", color="#00FF00"),
    dict(name="partial", description="Partially implemented", color="#80FF00"),
    dict(name="verified", description="Verified", color="#00A000"),
    dict(name="rejected", description="Rejected", color="#FF0000"),
]

# Valid tags
needs_tags = [
    dict(name="core", description="Core functionality"),
    dict(name="protocol", description="Protocol related"),
    dict(name="performance", description="Performance related"),
    dict(name="security", description="Security related"),
    dict(name="quality", description="Quality related"),
    dict(name="safety", description="Safety related"),
    dict(name="concurrency", description="Concurrency related"),
    dict(name="api", description="API related"),
    dict(name="observability", description="Observability related"),
    dict(name="telemetry", description="Telemetry related"),
    dict(name="integration", description="Integration related"),
    dict(name="ai", description="AI related"),
    dict(name="orchestration", description="Orchestration related"),
    dict(name="workflow", description="Workflow related"),
    dict(name="interface", description="Interface related"),
    dict(name="cli", description="CLI related"),
    dict(name="documentation", description="Documentation related"),
    dict(name="maintenance", description="Maintenance related"),
    dict(name="build", description="Build related"),
    dict(name="migration", description="Migration related"),
    dict(name="compatibility", description="Compatibility related"),
    dict(name="architecture", description="Architecture related"),
    dict(name="design", description="Design related"),
    dict(name="patterns", description="Patterns related"),
    dict(name="transport", description="Transport related"),
    dict(name="network", description="Network related"),
    dict(name="testing", description="Testing related"),
    dict(name="verification", description="Verification related"),
    dict(name="interaction", description="Interaction related"),
    dict(name="specification", description="Specification related"),
]

# Configuration options
needs_id_required = True
needs_title_optional = False
needs_file_pattern = "**/*.rst"
needs_show_link_type = True
needs_show_link_title = True
needs_table_style = "table"
needs_template_folder = "_needs_templates"

# Pattern to validate IDs
needs_id_regex = "^[A-Z0-9\\-]+"

source_suffix = {
    ".rst": "restructuredtext",
    ".md": "markdown",
    ".txt": "markdown",  # Optional
}

# See docs/compatibility for details on these extensions.
myst_enable_extensions = {
    "attrs_block",
    "colon_fence",
    "html_admonition",
    "replacements",
    "smartquotes",
    "strikethrough",
    "tasklist",
}
rust_crates = {
    "mcp-agent": "mcp-agent",
}
rust_doc_dir = "docs/source/"
rust_rustdoc_fmt = "md"
