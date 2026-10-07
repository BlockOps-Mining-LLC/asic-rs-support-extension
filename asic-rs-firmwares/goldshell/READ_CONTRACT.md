The read routes and field names come from pyasic 0.79 `web/goldshell.py`,
`miners/backends/goldshell.py`, and `miners/backends/bfgminer.py`.
The October 4, 2026 diagnostic inventory reported generic Goldshell identities
with firmware 2.2.0 and 2.2.3. It did not establish exact models or algorithms.

Fixture files are minimal contract examples, not captured live API responses.
The generic model and firmware in `status_contract.json` reproduce only those
non-sensitive inventory labels. Numeric telemetry examples exercise parser
semantics and must not be treated as hardware specifications or farm evidence.
Live validation is pending because the relevant farm network was inaccessible.

Only read RPC commands and GET telemetry routes are exposed. The HTTP client
may obtain a session using supplied/default credentials; it never logs out,
modifies credentials, changes configuration, or exposes any control capability.
Generic/unknown model identity preserves an unknown algorithm. Expected board,
chip, and fan counts remain absent. Reported working counts remain separate.
