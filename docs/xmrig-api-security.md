# XMRig API Security

The inspector derives host, port and token only from the inspected process's active JSON configuration. It accepts `127.0.0.0/8` and `::1`; wildcard binds are contacted through loopback with an exposure warning. Any resolution containing a non-loopback candidate is rejected.

Only `GET /2/summary` is issued over a direct TCP connection. Redirects are rejected, proxy environment variables are not consulted, and response size and time are bounded. Tokens are used only to construct the in-memory request and are never serialized.

