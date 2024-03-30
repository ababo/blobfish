# Blobfish

Blobfish is an AI-powered personal assistant.

## The Project Structure

The system consists of several components.

* `bfsrv` - a web-server that handles Blobfish API requests from various user agents. It is written in Rust.

* `infsrv` - an internal stateless web-server used by the `bfsrv`. The `infsrv` is written in Python and handles AI model inference requests.
