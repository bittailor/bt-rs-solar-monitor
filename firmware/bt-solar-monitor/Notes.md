Startup 


AT
-> OK

AT+CGDCONT=1,"IP","gprs.swisscom.ch"
-> OK

AT+CREG?
-> +CREG: 0,1
-> Ok

AT+HTTPINIT
-> OK

AT+HTTPPARA="URL","http://api.solar.bockmattli.ch/api/v1/solar"
-> OK

AT+HTTPACTION=0
-> OK

AT+HTTPREAD?
-> +HTTPREAD: LEN,10
-> OK

AT+HTTPREAD=0,10
-> OK
-> +HTTPREAD: 10
-> solar data
-> +HTTPREAD: 0

## LTE MQTT sketch

`nrf/apps/sketch/src/bin/lte-mqtt.rs` uses the same Xenon wiring as `lte.rs`
(115200 baud, RX P0.08, TX P0.06, PWRKEY P0.04, RESET P0.03).
It selects the Xenon's external high-frequency crystal for accurate UART timing.
It connects to the test EMQX broker over TLS on port 8883, subscribes to
`bt-solar-monitor/fw-test-one/loopback`, publishes a QoS 1 message, checks
the received topic and payload, unsubscribes, disconnects, releases the client,
and stops the MQTT service. The final RTT message reports pass/fail.

Set `MQTT_USERNAME` and `MQTT_PASSWORD` in your local environment (never commit
credentials). Set `MQTT_CA_CERT` to the **absolute path** of a trusted PEM root
CA for the broker. The sketch uploads it to the modem as `mqtt-ca.pem`.
For the current broker chain this is DigiCert Global Root G2, available from
<https://cacerts.digicert.com/DigiCertGlobalRootG2.crt.pem>; independently verify
the CA when changing brokers. Optionally set `MQTT_APN`; the default is
`gprs.swisscom.ch`. Existing `SOLAR_BACKEND_*` build variables are still required.

With these variables exported, build or flash and view RTT logs:

```sh
just --command sh -c 'cd nrf && cargo run --release --features lte-mqtt --bin lte-mqtt'
# Build only, without flashing:
just --command sh -c 'cd nrf && cargo build --release --features lte-mqtt --bin lte-mqtt'
```

The opt-in `lte-mqtt` feature keeps normal workspace builds independent of
the sketch's credential/certificate environment variables.

The client uses modem index 0, MQTT 3.1.1 and clean sessions. TLS enables TLS 1.2,
server CA authentication, SNI and certificate time checks; the modem's clock
must be correct. It never falls back to unauthenticated TLS. Credentials are
embedded in the resulting firmware image, but CONNECT commands and their echoes
are redacted from AT logs. Do not distribute that image.

The API is `no_std` with no allocator. Receive storage is fixed: two queued
messages, each with a 256-byte topic and 1024-byte binary payload. MQTT URCs
are assembled by byte length, including split topic/payload chunks and messages
arriving during other AT commands. Oversized messages are drained and reported
as `AtError::CapacityError`, as is queue overflow (not silent truncation).
Outgoing topics/payloads support the modem's 1024/10240-byte limits; empty
outgoing payloads are rejected because the documented AT command minimum is 1.
The AT command buffer is 256 bytes, including CONNECT's host and credentials;
oversized commands return a formatting error before starting the service.

Call `mqtt_disconnect()` explicitly, including after a partially failed connect.
Disconnect before modem sleep. On an AT timeout the modem's state may be
uncertain; power-cycle before reconnecting if cleanup fails. Like existing AT
operations, MQTT futures must not be externally cancelled; use the timeout
argument of `mqtt_receive()` instead.
Connecting MQTT disables UART sleep so incoming messages can be reported;
sleep is not automatically re-enabled on disconnect.