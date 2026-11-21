//! TLS MQTT loopback sketch. See firmware Notes.md for build-time configuration.
#![no_std]
#![no_main]

use bt_core::{
    at::AtController,
    net::cellular::{
        CellularError,
        sim_com_a67::{MqttConfig, MqttQos, MqttTlsConfig, SimComCellularModule},
    },
};
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join3;
use embassy_nrf::{
    bind_interrupts,
    buffered_uarte::{self, BufferedUarte},
    gpio::{Level, Output, OutputDrive},
    peripherals, uarte,
};
use embassy_time::{Duration, Timer};
use embedded_hal::digital::OutputPin;
use {defmt_rtt as _, panic_probe as _};

const HOST: &str = "v1f449fa.ala.eu-central-1.emqxsl.com";
const USERNAME: &str = env!("MQTT_USERNAME", "Set MQTT_USERNAME when building lte-mqtt");
const PASSWORD: &str = env!("MQTT_PASSWORD", "Set MQTT_PASSWORD when building lte-mqtt");
const CA: &[u8] = include_bytes!(env!("MQTT_CA_CERT", "Set MQTT_CA_CERT to the absolute path of the broker's PEM root CA"));
const TOPIC: &str = "bt-solar-monitor/fw-test-one/loopback";
const PAYLOAD: &[u8] = b"hello from Xenon LTE";

bind_interrupts!(struct Irqs {
    UARTE0 => buffered_uarte::InterruptHandler<peripherals::UARTE0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.hfclk_source = embassy_nrf::config::HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(nrf_config);
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);
    let reset = Output::new(p.P0_03, Level::Low, OutputDrive::Standard);
    let pwrkey = Output::new(p.P0_04, Level::Low, OutputDrive::Standard);
    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;
    let mut tx = [0; 1024];
    let mut rx = [0; 4096];
    let uart = BufferedUarte::new(p.UARTE0, p.TIMER0, p.PPI_CH0, p.PPI_CH1, p.PPI_GROUP0, p.P0_08, p.P0_06, Irqs, config, &mut rx, &mut tx);
    let mut state = bt_core::at::State::new();
    let (runner, client) = bt_core::at::new(&mut state, uart);
    let mut lte = SimComCellularModule::new(client, pwrkey, reset);

    let sequence = async {
        let result = mqtt_sequence(&mut lte).await;
        let cleanup = lte.mqtt_disconnect().await;
        if let Err(error) = &cleanup {
            error!("MQTT cleanup failed: {:?}", error);
        }
        match result.and(cleanup) {
            Ok(()) => info!("MQTT TLS subscription/publish round trip PASSED"),
            Err(error) => error!("MQTT sketch FAILED: {:?}", error),
        }
    };
    let blinky = async {
        loop {
            led.set_high();
            Timer::after_secs(1).await;
            led.set_low();
            Timer::after_secs(1).await;
        }
    };
    join3(runner.run(), blinky, sequence).await;
}

async fn mqtt_sequence(lte: &mut SimComCellularModule<'_, impl OutputPin, impl AtController>) -> Result<(), CellularError> {
    lte.power_cycle().await?;
    // Keep the timeout inside the registration loop; AT operations must not be cancelled.
    lte.set_apn(option_env!("MQTT_APN").unwrap_or("gprs.swisscom.ch")).await?;
    let deadline = embassy_time::Instant::now() + Duration::from_secs(180);
    while lte.read_network_registration().await?.1 != bt_core::at::network::NetworkRegistrationState::Registered {
        if embassy_time::Instant::now() >= deadline {
            error!("LTE network registration timed out");
            return Err(CellularError::Timeout);
        }
        Timer::after_secs(2).await;
    }
    let rtc = lte.query_real_time_clock().await?;
    info!("Modem clock (must be correct for TLS): {}", defmt::Display2Format(&rtc));
    lte.mqtt_upload_ca("mqtt-ca.pem", CA).await?;
    let config = MqttConfig {
        host: HOST,
        port: 8883,
        client_id: "xenon-fw-test-one",
        username: USERNAME,
        password: PASSWORD,
        keep_alive_secs: 60,
        tls: Some(MqttTlsConfig { ca_file: "mqtt-ca.pem" }),
    };
    lte.mqtt_connect(&config).await?;
    info!("MQTT TLS connected");
    lte.mqtt_subscribe(TOPIC, MqttQos::AtLeastOnce).await?;
    info!("MQTT subscribed");
    lte.mqtt_publish(TOPIC, PAYLOAD, MqttQos::AtLeastOnce, false).await?;
    info!("MQTT published");
    let message = lte.mqtt_receive(Duration::from_secs(60)).await?;
    if message.topic.as_slice() != TOPIC.as_bytes() || message.payload.as_slice() != PAYLOAD {
        error!("MQTT loopback topic/payload mismatch");
        return Err(bt_core::at::AtError::Error.into());
    }
    info!("MQTT received expected loopback payload");
    lte.mqtt_unsubscribe(TOPIC).await?;
    Ok(())
}
