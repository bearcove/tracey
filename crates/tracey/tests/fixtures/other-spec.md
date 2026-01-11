# Other Specification

This is a second specification for testing multi-spec prefix filtering.

## API

r[api.fetch]
The fetch function MUST return data from the remote server.

r[api.cache]
Responses SHOULD be cached for performance.

r[api.retry]
Failed requests MUST be retried with exponential backoff.
