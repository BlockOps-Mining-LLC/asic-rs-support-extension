The read API and field names follow pyasic 0.79 `web/innosilicon.py`,
`miners/backends/innosilicon.py`, and `miners/backends/cgminer.py`.
The October 4, 2026 farm inventory reported generic Innosilicon identities with
firmware `a9-1.2.0`; this is not proof of an exact A9 variant or algorithm.

Fixtures are minimal contract examples, not live captures. Numeric examples
exercise the parser and are not farm telemetry or hardware specifications.
No device IP, MAC, worker, pool address, credential, or token is retained.
Live validation remains pending because the relevant networks were inaccessible.

POST is the vendor's documented read transport for `type`, `getAll`, `overview`,
`summary`, `getErrorDetail`, and `pools`. Login obtains a read session only.
No restart, poweroff, configuration, password-change, or other write capability
is implemented. Explicitly named rate fields determine units, never firmware
version or manufacturer. Expected hardware remains unknown. Chip counts are
reported working counts, and fan percentages are not converted into assumed RPM.

Power provenance names the exact selected field: `/api/getAll#/all/power`, or
`stats#/STATS/<reported row index>/power` for the RPC fallback. A reported zero
is retained with its source. The API field alone does not establish whether
power is measured or estimated, so that classification remains unknown.
