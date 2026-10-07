# Mara / KaonSu telemetry

This crate reads Mara / KaonSu firmware through authenticated GET requests to
`/kaonsu/v1/brief`, `/overview`, `/hashboards`, and `/fans`. It does not read miner
configuration or advertise controls. Discovery requires a MaraFW or KaonSu
signature; a stock Antminer model name alone is insufficient.

The overview's exact product name selects the Antminer model and hash algorithm.
Known KS5 / KS5 Pro, L9, and S19k Pro models use KHeavyHash, Scrypt, and SHA256
respectively. Explicit rate units are preserved and converted to the algorithm's
standard unit. Endpoint defaults follow the existing firmware contract: aggregate
and realtime rates use TH/s; ideal and average board rates use GH/s. Unknown models
retain an unknown algorithm.

`power_consumption_estimated` remains an estimate, including zero. Its field path,
firmware `power_source`, and integer `power_indicator` are retained separately.
The miner-wide reported maximum temperature comes only from `brief.temperature_max`;
it is not copied onto a board or derived from an average. Board PCB and chip
temperatures retain their sensor domains. Actual chip counts include zero;
expected chip counts require `asic_num_ideal`, with no stock hardware substitution.

The sanitized fixtures test existing firmware contracts, including missing,
malformed, stopped, and fault readings. They do not establish live acceptance of
every firmware release or model. MAC, serial number, and uptime remain unknown
when the four permitted endpoints do not provide a proven field.
