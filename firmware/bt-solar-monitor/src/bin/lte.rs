#![no_std]
#![no_main]

use core::str::from_utf8;

use bt_solar_monitor::net::lte;
use bt_solar_monitor::net::lte::at_cmds::AtError;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Stack, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::UART0;
use embassy_rp::uart::{self, BufferedInterruptHandler, BufferedUart};
use embassy_time::{Duration, Timer, WithTimeout as _};
use heapless::Vec;
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::Method;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, embassy_net_ppp::Device<'static>>) -> ! {
    info!("net_task A");
    runner.run().await
}

#[embassy_executor::task]
async fn ppp_task(
    stack: Stack<'static>,
    mut runner: embassy_net_ppp::Runner<'static>,
    uart: BufferedUart,
) {
    //let port = Async::new(port).unwrap();
    //let port = BufReader::new(port);
    //let port = embedded_io_adapters::futures_03::FromFutures::new(port);

    info!("ppp_task A");

    let config = embassy_net_ppp::Config {
        username: b"myuser",
        password: b"mypass",
    };

    info!("ppp_task B");
    runner
        .run(uart, config, |ipv4| {
            let Some(addr) = ipv4.address else {
                warn!("PPP did not provide an IP address.");
                return;
            };
            let mut dns_servers = Vec::new();
            for s in ipv4.dns_servers.iter().flatten() {
                let _ = dns_servers.push(*s);
            }
            let config = embassy_net::ConfigV4::Static(embassy_net::StaticConfigV4 {
                address: embassy_net::Ipv4Cidr::new(addr, 0),
                gateway: None,
                dns_servers,
            });
            stack.set_config_v4(config);
        })
        .await
        .unwrap();
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);
    let mut reset = Output::new(p.PIN_16, Level::High);
    let mut _pwrkey = Output::new(p.PIN_17, Level::High);

    let mut rng = RoscRng;

    let (tx_pin, rx_pin, uart) = (p.PIN_0, p.PIN_1, p.UART0);

    static TX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let tx_buf = &mut TX_BUF.init([0; 1024])[..];
    static RX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let rx_buf = &mut RX_BUF.init([0; 1024])[..];
    let mut uart: BufferedUart = BufferedUart::new(
        uart,
        tx_pin,
        rx_pin,
        Irqs,
        tx_buf,
        rx_buf,
        uart::Config::default(),
    );

    // Init network device
    static STATE: StaticCell<embassy_net_ppp::State<4, 4>> = StaticCell::new();
    let state = STATE.init(embassy_net_ppp::State::<4, 4>::new());
    let (device, ppp_runner) = embassy_net_ppp::new(state);

    // Generate random seed
    let seed = rng.next_u64();

    // Init network stack
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let (stack, net_runner) = embassy_net::new(
        device,
        embassy_net::Config::default(), // don't configure IP yet
        RESOURCES.init(StackResources::new()),
        seed,
    );

    info!("... reset ...");
    reset.set_low();
    Timer::after_millis(2500).await;
    reset.set_high();
    _ = startup_lte(&mut uart).await;

    // Launch network task
    info!("spawn net_task");
    spawner.spawn(net_task(net_runner)).unwrap();
    info!("spawn ppp_task");
    spawner.spawn(ppp_task(stack, ppp_runner, uart)).unwrap();

    info!("... stack stack.wait_config_up ...");
    stack.wait_config_up().await;
    info!("... stack up ...");

    for _ in 1..5 {
        match stack
            .dns_query(
                "playground.bockmattli.ch",
                embassy_net::dns::DnsQueryType::A,
            )
            .await
        {
            Ok(ip) => info!("Resolved IP address: {}", ip),
            Err(e) => warn!("DNS query failed: {}", e),
        };
    }

    info!("... request ...");
    requests(stack, seed).await;
    info!("... done ...");

    /*
    let mut rx_buf = [0; 4096];
    let response = client
        .request(Method::POST, &url)
        .await
        .unwrap()
        .body(b"PING")
        .content_type(ContentType::TextPlain)
        .send(&mut rx_buf)
        .await
        .unwrap();
    */

    loop {
        led.set_high();
        Timer::after_millis(300).await;
        led.set_low();
        Timer::after_millis(300).await;
    }
}

pub async fn startup_lte(uart: &mut BufferedUart) -> Result<(), AtError> {
    info!("... flush ...");
    let flush = async {
        loop {
            match lte::at_cmds::read_response(uart).await {
                Ok(line) => info!("uart> {}", line),
                Err(e) => warn!("uart error> {}", e),
            }
        }
    };
    _ = flush.with_timeout(Duration::from_millis(5000)).await;
    info!("... flush done ...");

    info!("... startup ...");

    let mut client = lte::at_cmds::AtClient::new(uart);

    client.send_command("AT").await?;
    client.send_command("AT").await?;
    //client.send_command("ATE0").await?;

    client.send_request("AT+CSQ").await?;

    client
        .send_command("AT+CGDCONT=1,\"IP\",\"gprs.swisscom.ch\"")
        .await?;

    info!("... wait for roaming lte connection to be established ...");
    loop {
        let response = client.send_request("AT+CEREG?").await?;
        if response == "+CEREG: 0,1" {
            break;
        }
        Timer::after_millis(250).await;
    }
    info!("... roaming lte connection established ...");

    //client.send_command("AT+CSSLCFG=\"authmode\",0,0").await?;
    //client.send_command("AT+CSSLCFG=\"enableSNI\",0,1").await?;

    /*     let fast_baudrate = 460800;
    client.send_command("AT+IPR=460800").await?;
    client.rw.set_baudrate(fast_baudrate);
    client.rw.flush().await?;

    Timer::after_millis(1000).await;
    */

    client.send_command("ATD*99#").await?;

    /*
    client.send_line("ATD*99#").await?;

    info!("... flush ...");
    let flush = async {
        loop {
            match lte::at_cmds::read_response(uart).await {
                Ok(line) => info!("uart> {}", line),
                Err(e) => warn!("uart error> {}", e),
            }
        }
    };
    _ = flush.with_timeout(Duration::from_millis(5000)).await;
    info!("... flush done ...");
    */

    Ok(())
}

async fn requests(stack: Stack<'_>, seed: u64) {
    let mut tls_read_buffer = [0; 16640];
    let mut tls_write_buffer = [0; 16640];

    let client_state = TcpClientState::<1, 1024, 1024>::new();
    let tcp_client = TcpClient::new(stack, &client_state);
    let dns_client = DnsSocket::new(stack);
    let tls_config = TlsConfig::new(
        seed,
        &mut tls_read_buffer,
        &mut tls_write_buffer,
        TlsVerify::None,
    );

    //let url = format!("http://localhost", addr.port());
    let mut http_client = HttpClient::new_with_tls(&tcp_client, &dns_client, tls_config); // Types implementing embedded-nal-async

    request(
        "http://api.solar.bockmattli.ch/api/v1/solar",
        &mut http_client,
    )
    .await;
    request(
        "http://api.solar.bockmattli.ch/api/v1/lte",
        &mut http_client,
    )
    .await;
}

async fn request(url: &str, http_client: &mut HttpClient<'_, TcpClient<'_, 1>, DnsSocket<'_>>) {
    let mut rx_buffer = [0; 8192];
    info!("HTTP GET -> {}", &url);
    {
        let mut request = match http_client.request(Method::GET, &url).await {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to make HTTP request: {:?}", e);
                return; // handle the error
            }
        };

        let response = match request.send(&mut rx_buffer).await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Failed to send HTTP request {:?}", e);
                return; // handle the error;
            }
        };
        let body = match from_utf8(response.body().read_to_end().await.unwrap()) {
            Ok(b) => b,
            Err(_e) => {
                error!("Failed to read response body");
                return; // handle the error
            }
        };
        info!("Response body: {:?}", &body);
    }
}
