# Implement MQTT client capability on top of SimCom module

We have [Clipper LTE 4G Breakout](https://shop.pimoroni.com/en-ch/products/clipper-breakout?variant=42180720459859) module with a SIMCom A7683E 4G LTE module(https://en.simcom.com/product/A7683E.html) connected to a Particle Xenon board with a nRF52840 SoC. 

The firmware is created with Rust and Embassy as async executor.

The gaol of this task ist to extend the SimComCellularModule in components/bt-core/src/net/cellular/sim_com_a67.rs with for MQTT client based on the 'AT Commands for MQTT(S)' (see chapter 18 in the /Users/bittailor/Code/Bockmattli/bt-rs-solar-monitor/doc/A76XX_Series_AT_Command_Manual_V1_04.pdf).

We have a mqtt broker at 

v1f449fa.ala.eu-central-1.emqxsl.com with MQTT over TLS/SSL on port 8883 to test it with a user `fw-test-one` and password `fs-12-test-34-bt`

create a lte-mqtt.rs in nrf/apps/sketch/src/bin that is a sketch firmware that starts up the lte module and does a MQTT subscription and a publish. 

You can get inspired by the other firmware sketches in nrf/apps/sketch/src/bin folder.

Ensure we only use non_std no alloc rust as we build an embedded firmware.

Anything unclear ask back.
