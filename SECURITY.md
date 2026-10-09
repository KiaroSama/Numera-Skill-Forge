# Security

Treat imported manifests, Markdown, scripts and native modules as untrusted input. Discovery is
metadata-only; installation is not execution approval. New executable content or privilege changes
need renewed review. Never include credentials in repository URLs, packages, diffs or logs.

Report suspected vulnerabilities privately through the repository owner's contact route. Do not post
secret values or exploit user environments. This development preview has not received a full security
audit and is not a sandbox for arbitrary plugin execution.
