#!/bin/bash
# Script to convert PlantUML diagrams to PNG images

# Path to PlantUML jar file
# Note: You need to download this separately from http://plantuml.com/download
PLANTUML_JAR="plantuml.jar"

# Check if PlantUML jar exists
if [ ! -f "$PLANTUML_JAR" ]; then
    echo "PlantUML jar not found. Downloading..."
    curl -L -o plantuml.jar "https://github.com/plantuml/plantuml/releases/download/v1.2023.11/plantuml-1.2023.11.jar"
fi

# Directory with PlantUML files
PUML_DIR="_static"

# Create output directory if it doesn't exist
mkdir -p "$PUML_DIR"

# Process all PlantUML files
for file in "$PUML_DIR"/*.puml; do
    if [ -f "$file" ]; then
        echo "Processing $file..."
        java -jar "$PLANTUML_JAR" "$file"
    fi
done

echo "Diagram generation complete." 