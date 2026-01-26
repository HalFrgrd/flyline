#!/bin/bash

# Script to generate Ubuntu Dockerfiles from a template
# Usage: ./generate_dockerfiles.sh

TEMPLATE_DIR="tests/docker_integration_tests"
TEMPLATE_FILE="$TEMPLATE_DIR/Dockerfile.ubuntu.template"

# Ubuntu versions to generate
VERSIONS=("18.04" "20.04" "22.04" "24.04")

echo "Generating Ubuntu Dockerfiles from template..."

for version in "${VERSIONS[@]}"; do
    output_file="$TEMPLATE_DIR/Dockerfile.ubuntu.$version"
    
    # Replace UBUNTU_VERSION placeholder in template
    sed "s/\${UBUNTU_VERSION}/$version/g" "$TEMPLATE_FILE" > "$output_file"
    
    # Update the comment to reflect the specific version
    sed -i "1s/.*/# Test with Ubuntu $version/" "$output_file"
    
    echo "Generated: $output_file"
done

echo "Done! Generated Dockerfiles for Ubuntu versions: ${VERSIONS[*]}"