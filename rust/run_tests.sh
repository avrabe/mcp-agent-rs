#!/bin/bash

# Run library, binary, and test suite tests
cargo test --lib --bins --tests --benches

# Run all examples tests one by one, excluding simple_visualization
for example in basic_workflow conditional_workflow config_example dependency_workflow dual_terminal event_driven_workflow fan_out_fan_in_workflow graph_output_example graph_vis human_input_example jsonrpc_example minimal_vis orchestrator_workflow parallel_workflow pipeline_workflow retry_workflow router_workflow saga_workflow simple_client simple_workflow_runner state_machine_workflow visualize_workflow workflow_visualizer
do
  echo "Testing example: $example"
  cargo test --example $example
done

echo "All tests complete!" 