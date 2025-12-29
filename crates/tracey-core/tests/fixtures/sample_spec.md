# Sample Specification

This is a sample specification document for testing tracey's markdown processor.

## Channel Management

r[channel.id.allocation]
Channel IDs MUST be allocated sequentially starting from 0.

r[channel.id.parity]
Client-initiated channels MUST use odd IDs, server-initiated channels MUST use even IDs.

### Channel Lifecycle

r[channel.lifecycle.open]
A channel MUST be explicitly opened before any data can be sent.

r[channel.lifecycle.close]
When a channel is closed, all pending operations MUST be cancelled.

## Error Handling

r[error.codes.range]
Error codes MUST be in the range 0-65535.

r[error.propagation]
Errors MUST propagate to all affected channels within 100ms.

## Performance Requirements

r[perf.latency.p99]
The 99th percentile latency for message delivery MUST be under 10ms.

r[perf.throughput.minimum]
The system MUST support at least 10,000 messages per second per connection.
