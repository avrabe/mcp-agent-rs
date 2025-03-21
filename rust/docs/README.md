# MCP-Agent Documentation

This directory contains the source files for the MCP-Agent documentation.

## Building the Documentation

To build the documentation, you need [Sphinx](https://www.sphinx-doc.org/) installed:

```bash
# Install dependencies
pip install -r requirements.txt

# Build the documentation
cd /path/to/mcp-agent/rust/doc
sphinx-build -b html source _build/html
```

The built documentation will be available in the `_build/html` directory.

## Generating Diagrams

The architecture documentation uses PlantUML diagrams. To generate the PNG images from the PlantUML source files:

1. Make sure you have Java installed
2. Run the diagram generation script:

```bash
cd /path/to/mcp-agent/rust/doc
./generate_diagrams.sh
```

The script will:

- Download the PlantUML jar file if it's not present
- Process all `.puml` files in the `_static` directory
- Generate PNG images in the same directory

## Structure

- `source/`: Documentation source files
- `_static/`: Static files (images, diagrams, etc.)
- `_build/`: Generated documentation (after building)
- `requirements.txt`: Python dependencies for building the documentation
- `generate_diagrams.sh`: Script to convert PlantUML to PNG images

## Contributing

When adding new documentation:

1. Add new RST files to the `source/` directory
2. Update the table of contents in `source/index.rst`
3. For new diagrams, add PlantUML files to `_static/` and run the generation script
4. Build the documentation locally to verify changes

## License

The documentation is licensed under the same license as the MCP-Agent project.
