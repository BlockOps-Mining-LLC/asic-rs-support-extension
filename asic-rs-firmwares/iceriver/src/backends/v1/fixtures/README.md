The AL3 userpanel fixture is synthetic and follows the read-only contract in
pyasic 0.79's IceRiver web, backend, factory, and model implementations. It is
not a live device capture. It intentionally omits network identity and pool
configuration.

The vendor contract uses POST `/user/userpanel` with `post=4` after a login
session. `data.softver1` supplies model identity; suffix `10306` maps to AL3.
The fixture exercises `unit`, `rtpow`, `powstate`, `runtime`, `locate`, `fans`,
and hashboard `no`, `intmp`, `outtmp`, `rtpow`, and `chipnum` fields. The
`intmp`/`outtmp` sensor roles follow pyasic's board/chip mapping and have not
been independently checked on reachable IceRiver hardware.

No measured wattage, power estimate, per-chip temperatures, control-board
identity, firmware error messages, or detailed operating state is established
by this source contract. Such readings stay unavailable. `powstate` supplies
the separate `is_mining` flag; it does not fabricate a detailed state.
