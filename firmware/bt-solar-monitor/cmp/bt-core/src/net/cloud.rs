use embassy_futures::yield_now;

use crate::net::cellular::{CellularError, CellularModule};

pub struct Runner<Module: CellularModule> {
    cloud_client: CloudClient<Module>,
}

pub fn new<Module: CellularModule>(module: Module) -> Runner<Module> {
    Runner {
        cloud_client: CloudClient {
            module,
            state: CloudClientState::Startup,
        },
    }
}

impl<Module: CellularModule> Runner<Module> {
    pub async fn run(mut self) {
        loop {
            self.cloud_client.once().await;
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum CloudClientState {
    Startup,
    Connected,
    Sleeping,
}

pub struct CloudClient<Module: CellularModule> {
    module: Module,
    state: CloudClientState,
}
impl<Module: CellularModule> CloudClient<Module> {
    pub async fn sleep(&mut self) -> Result<(), CellularError> {
        //self.module.set_sleep_mode(SleepMode::Enabled).await?;
        self.state = CloudClientState::Sleeping;
        Ok(())
    }

    async fn once(&mut self) {
        let result = match self.state {
            CloudClientState::Startup => self.handle_startup().await,
            CloudClientState::Connected => self.handle_connected().await,
            CloudClientState::Sleeping => self.handle_sleeping().await,
        };
        if let Err(e) = result {
            warn!("CloudClient error: {:?}", e);
            // TODO: handle error and state transitions
        }
    }

    async fn handle_startup(&mut self) -> Result<(), CellularError> {
        self.module.power_cycle().await?;
        self.module.startup_network("gprs.swisscom.ch").await?;
        let now = self.module.query_real_time_clock().await?;
        crate::time::time_sync(now).await;
        self.state = CloudClientState::Connected;
        info!("CloudClient connected at {}", crate::fmt::FormatableNaiveDateTime(&now));
        Ok(())
    }

    async fn handle_connected(&mut self) -> Result<(), CellularError> {
        // TODO implement
        yield_now().await;
        Ok(())
    }

    async fn handle_sleeping(&mut self) -> Result<(), CellularError> {
        // TODO implement
        yield_now().await;
        Ok(())
    }
}

/*
impl<'ch, Output: OutputPin, Ctr: AtController> CloudClient<'ch, Output, Ctr> {
    pub async fn new(mut module: sim_com_a67::CellularModule<'ch, Output, Ctr>) -> Result<Self, CellularError> {
        module.power_cycle().await?;

        let new = Self { module };
        Ok(new)
    }

    async fn
}
*/
