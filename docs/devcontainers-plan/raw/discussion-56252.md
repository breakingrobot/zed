# Discussion #56252 — Support connecting to remote devcontainers (devcontainer on remote host)

Source: https://github.com/zed-industries/zed/discussions/56252 (fetched 2026-09-22 via web page; GraphQL unavailable without gh auth).
Summary produced by WebFetch — not verbatim. Verbatim GraphQL dump (after `gh auth`, 2026-09-23): `discussion-56252.json`.

- Author: lieeesson, 2026-05-09, converted from issue #56242.
- Request: connect to dev containers on a remote host over SSH (either discover via SSH, or configure a remote Docker host).
  Tried SSH port-forwarding of the Docker socket; Zed rejected the non-local socket.
- Replies: SpiderWhisperer (05-22), pahenrus (06-07), josiahls (06-07), JonathonRP (06-13) — demand/parity with VS Code.
- macraig (authorAssociation **CONTRIBUTOR** per GraphQL — staff status NOT verified, 2026-06-17): "Created an issue to track this here: #59500"
- borja-rojo-ilvento (08-13): notes VS Code needs specialized SSH socket plumbing.
- jepirat (09-08): also wants network routing through the container for extension downloads.
