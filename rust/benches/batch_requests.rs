//! Benchmarks for JSON-RPC batch vs. individual requests
//!
//! This benchmark compares the performance of:
//! 1. Sending multiple individual JSON-RPC requests
//! 2. Sending the same requests as a single batch
//!
//! The benchmark measures:
//! - Throughput (requests per second)
//! - Latency (time to receive responses)
//! - CPU usage

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
use mcp_agent::mcp::types::{JsonRpcBatchRequest, JsonRpcRequest};
use serde_json::json;
use std::time::Duration;
use tokio::runtime::Runtime;

// Helper function to create a configured JSON-RPC handler
async fn create_test_handler() -> JsonRpcHandler {
    let handler = JsonRpcHandler::new();

    // Register echo method
    handler
        .register_method("echo", |params| Ok(params.unwrap_or(json!(null))))
        .await
        .unwrap();

    // Register add method
    handler
        .register_method("add", |params| {
            let params = params.unwrap_or(json!([]));
            let params = params.as_array().unwrap();
            let a = params[0].as_i64().unwrap_or(0);
            let b = params[1].as_i64().unwrap_or(0);
            Ok(json!(a + b))
        })
        .await
        .unwrap();

    handler
}

// Benchmark sending individual requests
fn bench_individual_requests(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_rpc_requests");
    group.measurement_time(Duration::from_secs(10));

    // Create runtime and handler
    let rt = Runtime::new().unwrap();
    let handler = rt.block_on(create_test_handler());

    for num_requests in [1, 5, 10, 25, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("individual", num_requests),
            num_requests,
            |b, &num_requests| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut futures = Vec::with_capacity(num_requests);

                        // Create and send individual requests
                        for i in 0..num_requests {
                            let req = JsonRpcRequest::new(
                                "echo",
                                Some(json!(format!("test message {}", i))),
                                json!(i.to_string()),
                            );

                            futures.push(handler.handle_request(req));
                        }

                        futures::future::join_all(futures).await
                    })
                })
            },
        );
    }

    group.finish();
}

// Benchmark sending batch requests
fn bench_batch_requests(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_rpc_requests");
    group.measurement_time(Duration::from_secs(10));

    // Create runtime and handler
    let rt = Runtime::new().unwrap();
    let handler = rt.block_on(create_test_handler());

    for num_requests in [1, 5, 10, 25, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("batch", num_requests),
            num_requests,
            |b, &num_requests| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut batch = JsonRpcBatchRequest::new();

                        // Create batch request
                        for i in 0..num_requests {
                            let req = JsonRpcRequest::new(
                                "echo",
                                Some(json!(format!("test message {}", i))),
                                json!(i.to_string()),
                            );
                            batch.add_request(req);
                        }

                        // Send batch and get response
                        let response = handler.handle_batch_request(batch).await;
                        black_box(response)
                    })
                })
            },
        );
    }

    group.finish();
}

// Benchmark mixed workload (some requests more expensive than others)
fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_rpc_mixed_workload");
    group.measurement_time(Duration::from_secs(10));

    // Create runtime and handler
    let rt = Runtime::new().unwrap();
    let handler = rt.block_on(create_test_handler());

    for num_requests in [10, 50].iter() {
        // Benchmark individual requests
        group.bench_with_input(
            BenchmarkId::new("individual_mixed", num_requests),
            num_requests,
            |b, &num_requests| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut futures = Vec::with_capacity(num_requests);

                        // Create and send individual requests (mix of echo and add)
                        for i in 0..num_requests {
                            let req = if i % 2 == 0 {
                                // Echo request
                                JsonRpcRequest::new(
                                    "echo",
                                    Some(json!(format!("test message {}", i))),
                                    json!(i.to_string()),
                                )
                            } else {
                                // Add request
                                JsonRpcRequest::new(
                                    "add",
                                    Some(json!([i, i * 2])),
                                    json!(i.to_string()),
                                )
                            };

                            futures.push(handler.handle_request(req));
                        }

                        // Wait for all responses
                        futures::future::join_all(futures).await
                    })
                })
            },
        );

        // Benchmark batch requests
        group.bench_with_input(
            BenchmarkId::new("batch_mixed", num_requests),
            num_requests,
            |b, &num_requests| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut batch = JsonRpcBatchRequest::new();

                        // Create batch request (mix of echo and add)
                        for i in 0..num_requests {
                            let req = if i % 2 == 0 {
                                // Echo request
                                JsonRpcRequest::new(
                                    "echo",
                                    Some(json!(format!("test message {}", i))),
                                    json!(i.to_string()),
                                )
                            } else {
                                // Add request
                                JsonRpcRequest::new(
                                    "add",
                                    Some(json!([i, i * 2])),
                                    json!(i.to_string()),
                                )
                            };

                            batch.add_request(req);
                        }

                        // Send batch and get response
                        let response = handler.handle_batch_request(batch).await;
                        black_box(response)
                    })
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_individual_requests,
    bench_batch_requests,
    bench_mixed_workload
);
criterion_main!(benches);
